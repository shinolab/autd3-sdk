`timescale 1ns / 1ps
`default_nettype none
module flash_spi #(
    parameter int POLL_INTERVAL = 20480,
    parameter int POLL_LIMIT = 5000
) (
    input wire CLK,
    input wire START,
    input wire CMD_VALID,
    output var CMD_ACCEPT,
    input wire [7:0] CMD_OP,
    input wire [23:0] CMD_ADDR,
    input wire [23:0] CMD_LEN,
    output var [9:0] BUF_IDX,
    input wire [7:0] BUF_BYTE,
    output var DONE,
    output var [7:0] ERR,
    output var [31:0] RESULT,
    output var REBOOT,
    output var SCK,
    output var CS_N,
    output var MOSI,
    input wire MISO
);

  import params::*;

  localparam logic [7:0] SPI_RDID = 8'h9F;
  localparam logic [7:0] SPI_RDSR = 8'h05;
  localparam logic [7:0] SPI_READ = 8'h03;
  localparam logic [7:0] SPI_WREN = 8'h06;
  localparam logic [7:0] SPI_SE = 8'hD8;
  localparam logic [7:0] SPI_PP = 8'h02;

  localparam logic [24:0] SectorBytes = 25'h10000;
  localparam int StartupBits = 8;

  typedef enum logic [4:0] {
    WAIT_START,
    STARTUP,
    IDLE,
    SELECT,
    SEND,
    RECV,
    WRITE,
    DESELECT,
    ERASE_WREN,
    ERASE_CMD,
    PROGRAM_WREN,
    PROGRAM_CMD,
    POLL,
    POLL_CHECK,
    POLL_WAIT,
    NEXT,
    FINISH
  } state_t;

  state_t state = WAIT_START;
  state_t ret = IDLE;

  logic [1:0] div = 2'd0;
  logic [5:0] bit_cnt = 6'd0;
  logic [5:0] tx_bits = 6'd0;
  logic [31:0] tx = 32'd0;
  logic [7:0] rx = 8'd0;
  logic [23:0] rx_left = 24'd0;
  logic [8:0] wr_left = 9'd0;
  logic [7:0] wr_byte = 8'd0;
  logic [9:0] buf_idx = 10'd0;
  logic [23:0] addr_cur = 24'd0;
  logic [23:0] len_left = 24'd0;
  logic [8:0] sectors_left = 9'd0;
  logic [15:0] polls = 16'd0;
  logic [15:0] wait_cnt = 16'd0;
  logic [7:0] op = 8'd0;
  logic [7:0] err = FLASH_ERR_NONE;
  logic [31:0] crc = 32'hFFFFFFFF;
  logic [23:0] received = 24'd0;
  logic [31:0] result = 32'd0;
  logic miso_q = 1'b0;
  logic accept = 1'b0;
  logic done = 1'b0;
  logic reboot = 1'b0;
  logic sck = 1'b0;
  logic cs_n = 1'b1;
  logic mosi = 1'b0;

  logic [24:0] cmd_end;
  logic [24:0] erase_span;
  logic cmd_in_flash;
  logic cmd_writable;
  logic [8:0] page_room;
  logic [8:0] page_bytes;
  logic erase_target_ok;
  logic program_target_ok;

  assign CMD_ACCEPT = accept;
  assign BUF_IDX = buf_idx;
  assign DONE = done;
  assign ERR = err;
  assign RESULT = result;
  assign REBOOT = reboot;
  assign SCK = sck;
  assign CS_N = cs_n;
  assign MOSI = mosi;

  function automatic logic [31:0] crc32_update(input logic [31:0] c, input logic [7:0] d);
    logic [31:0] x;
    x = c ^ {24'd0, d};
    for (int i = 0; i < 8; i++) begin
      x = x[0] ? ((x >> 1) ^ 32'hEDB88320) : (x >> 1);
    end
    return x;
  endfunction

  assign cmd_end = {1'b0, CMD_ADDR} + {1'b0, CMD_LEN};
  assign erase_span = {9'd0, CMD_ADDR[15:0]} + {1'b0, CMD_LEN} + 25'hFFFF;
  assign cmd_in_flash = cmd_end <= FlashEnd;
  assign cmd_writable = (CMD_ADDR >= FlashWritableBase) && cmd_in_flash;
  assign page_room = 9'h100 - {1'b0, addr_cur[7:0]};
  assign page_bytes = (len_left > {15'd0, page_room}) ? page_room : len_left[8:0];
  assign erase_target_ok = (addr_cur >= FlashWritableBase) && (({1'b0, addr_cur} + SectorBytes) <= FlashEnd);
  assign program_target_ok = (addr_cur >= FlashWritableBase) && (({1'b0, addr_cur} + {16'd0, page_bytes}) <= FlashEnd);

  always_ff @(posedge CLK) begin
    miso_q <= MISO;
    accept <= 1'b0;
    done   <= 1'b0;
    reboot <= 1'b0;
    case (state)
      WAIT_START: begin
        if (START) begin
          div <= 2'd0;
          bit_cnt <= 6'd0;
          state <= STARTUP;
        end
      end
      STARTUP: begin
        div <= div + 2'd1;
        sck <= (div == 2'd1) || (div == 2'd2);
        if (div == 2'd3) begin
          bit_cnt <= bit_cnt + 6'd1;
          if (bit_cnt == 6'(StartupBits - 1)) begin
            state <= IDLE;
          end
        end
      end
      IDLE: begin
        sck <= 1'b0;
        div <= 2'd0;
        bit_cnt <= 6'd0;
        if (CMD_VALID) begin
          accept <= 1'b1;
          op <= CMD_OP;
          crc <= 32'hFFFFFFFF;
          received <= 24'd0;
          err <= FLASH_ERR_NONE;
          polls <= 16'd0;
          rx_left <= 24'd0;
          wr_left <= 9'd0;
          buf_idx <= 10'd0;
          ret <= FINISH;
          state <= FINISH;
          case (CMD_OP)
            FLASH_OP_READ_ID: begin
              tx <= {SPI_RDID, 24'd0};
              tx_bits <= 6'd8;
              rx_left <= 24'd3;
              state <= SELECT;
            end
            FLASH_OP_CRC32: begin
              tx <= {SPI_READ, CMD_ADDR};
              tx_bits <= 6'd32;
              rx_left <= CMD_LEN;
              if (!cmd_in_flash) begin
                err <= FLASH_ERR_INVALID;
              end else if (CMD_LEN != 24'd0) begin
                state <= SELECT;
              end
            end
            FLASH_OP_ERASE: begin
              addr_cur <= {CMD_ADDR[23:16], 16'h0000};
              sectors_left <= erase_span[24:16];
              if (!cmd_writable) begin
                err <= FLASH_ERR_PROTECTED;
              end else if (CMD_LEN != 24'd0) begin
                state <= ERASE_WREN;
              end
            end
            FLASH_OP_PROGRAM: begin
              addr_cur <= CMD_ADDR;
              len_left <= CMD_LEN;
              if (CMD_LEN > 24'(FlashBufBytes)) begin
                err <= FLASH_ERR_INVALID;
              end else if (!cmd_writable) begin
                err <= FLASH_ERR_PROTECTED;
              end else if (CMD_LEN != 24'd0) begin
                state <= PROGRAM_WREN;
              end
            end
            FLASH_OP_REBOOT: begin
              reboot <= 1'b1;
            end
            default: begin
              err <= FLASH_ERR_INVALID;
            end
          endcase
        end
      end
      SELECT: begin
        cs_n <= 1'b0;
        div  <= div + 2'd1;
        if (div == 2'd3) begin
          state <= SEND;
        end
      end
      SEND: begin
        div <= div + 2'd1;
        sck <= (div == 2'd1) || (div == 2'd2);
        if (div == 2'd0) begin
          mosi <= tx[31];
          tx   <= {tx[30:0], 1'b0};
        end
        if (div == 2'd3) begin
          bit_cnt <= bit_cnt + 6'd1;
          if (bit_cnt == tx_bits - 6'd1) begin
            bit_cnt <= 6'd0;
            if (rx_left != 24'd0) begin
              state <= RECV;
            end else if (wr_left != 9'd0) begin
              state <= WRITE;
            end else begin
              state <= DESELECT;
            end
          end
        end
      end
      RECV: begin
        div <= div + 2'd1;
        sck <= (div == 2'd1) || (div == 2'd2);
        if (div == 2'd0) begin
          mosi <= 1'b0;
        end
        if (div == 2'd2) begin
          rx <= {rx[6:0], miso_q};
        end
        if (div == 2'd3) begin
          bit_cnt <= bit_cnt + 6'd1;
          if (bit_cnt == 6'd7) begin
            bit_cnt <= 6'd0;
            received <= {received[15:0], rx};
            crc <= crc32_update(crc, rx);
            rx_left <= rx_left - 24'd1;
            if (rx_left == 24'd1) begin
              state <= DESELECT;
            end
          end
        end
      end
      WRITE: begin
        div <= div + 2'd1;
        sck <= (div == 2'd1) || (div == 2'd2);
        if (div == 2'd0) begin
          if (bit_cnt == 6'd0) begin
            mosi <= BUF_BYTE[7];
            wr_byte <= {BUF_BYTE[6:0], 1'b0};
            buf_idx <= buf_idx + 10'd1;
          end else begin
            mosi <= wr_byte[7];
            wr_byte <= {wr_byte[6:0], 1'b0};
          end
        end
        if (div == 2'd3) begin
          bit_cnt <= bit_cnt + 6'd1;
          if (bit_cnt == 6'd7) begin
            bit_cnt <= 6'd0;
            wr_left <= wr_left - 9'd1;
            if (wr_left == 9'd1) begin
              state <= DESELECT;
            end
          end
        end
      end
      DESELECT: begin
        sck  <= 1'b0;
        mosi <= 1'b0;
        cs_n <= 1'b1;
        div  <= div + 2'd1;
        if (div == 2'd3) begin
          state <= ret;
        end
      end
      ERASE_WREN: begin
        if (!erase_target_ok) begin
          err   <= FLASH_ERR_PROTECTED;
          state <= FINISH;
        end else begin
          tx <= {SPI_WREN, 24'd0};
          tx_bits <= 6'd8;
          ret <= ERASE_CMD;
          state <= SELECT;
        end
      end
      ERASE_CMD: begin
        tx <= {SPI_SE, addr_cur};
        tx_bits <= 6'd32;
        addr_cur <= addr_cur + 24'h010000;
        sectors_left <= sectors_left - 9'd1;
        ret <= POLL;
        state <= SELECT;
      end
      PROGRAM_WREN: begin
        if (!program_target_ok) begin
          err   <= FLASH_ERR_PROTECTED;
          state <= FINISH;
        end else begin
          tx <= {SPI_WREN, 24'd0};
          tx_bits <= 6'd8;
          ret <= PROGRAM_CMD;
          state <= SELECT;
        end
      end
      PROGRAM_CMD: begin
        tx <= {SPI_PP, addr_cur};
        tx_bits <= 6'd32;
        wr_left <= page_bytes;
        addr_cur <= addr_cur + {15'd0, page_bytes};
        len_left <= len_left - {15'd0, page_bytes};
        ret <= POLL;
        state <= SELECT;
      end
      POLL: begin
        polls <= polls + 16'd1;
        tx <= {SPI_RDSR, 24'd0};
        tx_bits <= 6'd8;
        rx_left <= 24'd1;
        ret <= POLL_CHECK;
        state <= SELECT;
      end
      POLL_CHECK: begin
        wait_cnt <= 16'd0;
        if (!received[0]) begin
          state <= NEXT;
        end else if (polls >= 16'(POLL_LIMIT)) begin
          err   <= FLASH_ERR_TIMEOUT;
          state <= FINISH;
        end else begin
          state <= POLL_WAIT;
        end
      end
      POLL_WAIT: begin
        wait_cnt <= wait_cnt + 16'd1;
        if (wait_cnt == 16'(POLL_INTERVAL - 1)) begin
          state <= POLL;
        end
      end
      NEXT: begin
        polls <= 16'd0;
        ret   <= FINISH;
        if ((op == FLASH_OP_ERASE) && (sectors_left != 9'd0)) begin
          state <= ERASE_WREN;
        end else if ((op == FLASH_OP_PROGRAM) && (len_left != 24'd0)) begin
          state <= PROGRAM_WREN;
        end else begin
          state <= FINISH;
        end
      end
      FINISH: begin
        if (op == FLASH_OP_CRC32) begin
          result <= ~crc;
        end else if (op == FLASH_OP_READ_ID) begin
          result <= {8'd0, received};
        end else begin
          result <= 32'd0;
        end
        done  <= 1'b1;
        state <= IDLE;
      end
      default: begin
        state <= IDLE;
      end
    endcase
  end

endmodule
`default_nettype wire
