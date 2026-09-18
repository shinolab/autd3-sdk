`timescale 1ns / 1ps
`default_nettype none
module flash_probe_core #(
    parameter int POLL_INTERVAL = 25600,
    parameter int POLL_LIMIT = 5000
) (
    input wire CLK,
    input wire START,
    input wire CMD_VALID,
    input wire [7:0] CMD_OP,
    input wire [23:0] CMD_ADDR,
    input wire [31:0] CMD_LEN,
    output var [63:0] RESULT,
    output var [7:0] RESULT_OP,
    output var BUSY,
    output var DONE,
    output var SCK,
    output var CS_N,
    output var MOSI,
    input wire MISO,
    output var IPROG_START,
    output var IPROG_REBOOT,
    output var [23:0] IPROG_ADDR,
    output var [31:0] IPROG_TIMER
);

  localparam logic [7:0] OP_ID = 8'h01;
  localparam logic [7:0] OP_SR = 8'h02;
  localparam logic [7:0] OP_READ = 8'h03;
  localparam logic [7:0] OP_CRC = 8'h04;
  localparam logic [7:0] OP_FSR = 8'h05;
  localparam logic [7:0] OP_ERASE = 8'h06;
  localparam logic [7:0] OP_PROGRAM = 8'h07;
  localparam logic [7:0] OP_IPROG = 8'h08;
  localparam logic [7:0] OP_WBSTAR = 8'h09;

  localparam logic [7:0] SPI_RDID = 8'h9F;
  localparam logic [7:0] SPI_RDSR = 8'h05;
  localparam logic [7:0] SPI_RDFSR = 8'h70;
  localparam logic [7:0] SPI_READ = 8'h03;
  localparam logic [7:0] SPI_WREN = 8'h06;
  localparam logic [7:0] SPI_SE4K = 8'h20;
  localparam logic [7:0] SPI_PP = 8'h02;

  localparam logic [23:0] WRITABLE_BASE = 24'h800000;
  localparam logic [32:0] FLASH_END = 33'h1000000;

  localparam logic [7:0] ERR_NONE = 8'h00;
  localparam logic [7:0] ERR_PROTECTED = 8'h01;
  localparam logic [7:0] ERR_TIMEOUT = 8'h02;

  localparam int STARTUP_BITS = 8;

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
    MAKE_RESULT,
    FINISH
  } state_t;

  state_t state = WAIT_START;
  state_t ret = IDLE;

  logic [1:0] div = 2'd0;
  logic [5:0] bit_cnt = 6'd0;
  logic [5:0] tx_bits = 6'd0;
  logic [31:0] tx = 32'd0;
  logic [7:0] rx = 8'd0;
  logic [31:0] rx_left = 32'd0;
  logic [8:0] wr_left = 9'd0;
  logic [7:0] wr_byte = 8'd0;
  logic [31:0] prng = 32'd1;
  logic [23:0] addr_cur = 24'd0;
  logic [23:0] len_left = 24'd0;
  logic [12:0] sectors_left = 13'd0;
  logic [15:0] polls = 16'd0;
  logic [15:0] wait_cnt = 16'd0;
  logic [7:0] err = ERR_NONE;
  logic [31:0] crc = 32'hFFFFFFFF;
  logic [63:0] received = 64'd0;
  logic [63:0] result = 64'd0;
  logic [7:0] op = 8'd0;
  logic miso_q = 1'b0;
  logic busy = 1'b1;
  logic done = 1'b0;
  logic sck = 1'b0;
  logic cs_n = 1'b1;
  logic mosi = 1'b0;
  logic iprog_start = 1'b0;
  logic iprog_reboot = 1'b0;
  logic [23:0] iprog_addr = 24'd0;
  logic [31:0] iprog_timer = 32'd0;

  logic [31:0] prng_next;
  logic [31:0] seed_mix;
  logic [8:0] page_room;
  logic [8:0] page_bytes;
  logic erase_allowed;
  logic program_allowed;

  assign RESULT = result;
  assign RESULT_OP = op;
  assign BUSY = busy;
  assign DONE = done;
  assign SCK = sck;
  assign CS_N = cs_n;
  assign MOSI = mosi;
  assign IPROG_START = iprog_start;
  assign IPROG_REBOOT = iprog_reboot;
  assign IPROG_ADDR = iprog_addr;
  assign IPROG_TIMER = iprog_timer;

  function automatic logic [31:0] crc32_update(input logic [31:0] c, input logic [7:0] d);
    logic [31:0] x;
    x = c ^ {24'd0, d};
    for (int i = 0; i < 8; i++) begin
      x = x[0] ? ((x >> 1) ^ 32'hEDB88320) : (x >> 1);
    end
    return x;
  endfunction

  function automatic logic [31:0] xorshift32(input logic [31:0] s);
    logic [31:0] x;
    x = s;
    x = x ^ (x << 13);
    x = x ^ (x >> 17);
    x = x ^ (x << 5);
    return x;
  endfunction

  assign prng_next = xorshift32(prng);
  assign seed_mix = {CMD_LEN[31:24], CMD_ADDR} ^ 32'h9E3779B9;
  assign page_room = 9'h100 - {1'b0, addr_cur[7:0]};
  assign page_bytes = ({15'd0, page_room} > len_left) ? len_left[8:0] : page_room;
  assign erase_allowed = (CMD_ADDR >= WRITABLE_BASE) && (({9'd0, CMD_ADDR} + {1'b0, CMD_LEN}) <= FLASH_END);
  assign program_allowed = (CMD_ADDR >= WRITABLE_BASE) && (({9'd0, CMD_ADDR} + {9'd0, CMD_LEN[23:0]}) <= FLASH_END);

  always_ff @(posedge CLK) begin
    miso_q <= MISO;
    iprog_start <= 1'b0;
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
          if (bit_cnt == STARTUP_BITS - 1) begin
            busy  <= 1'b0;
            state <= IDLE;
          end
        end
      end
      IDLE: begin
        sck <= 1'b0;
        div <= 2'd0;
        bit_cnt <= 6'd0;
        if (CMD_VALID) begin
          op <= CMD_OP;
          crc <= 32'hFFFFFFFF;
          received <= 64'd0;
          err <= ERR_NONE;
          polls <= 16'd0;
          rx_left <= 32'd0;
          wr_left <= 9'd0;
          busy <= 1'b1;
          done <= 1'b0;
          ret <= MAKE_RESULT;
          case (CMD_OP)
            OP_ID: begin
              tx <= {SPI_RDID, 24'd0};
              tx_bits <= 6'd8;
              rx_left <= 32'd3;
              state <= SELECT;
            end
            OP_SR: begin
              tx <= {SPI_RDSR, 24'd0};
              tx_bits <= 6'd8;
              rx_left <= 32'd1;
              state <= SELECT;
            end
            OP_FSR: begin
              tx <= {SPI_RDFSR, 24'd0};
              tx_bits <= 6'd8;
              rx_left <= 32'd1;
              state <= SELECT;
            end
            OP_READ: begin
              tx <= {SPI_READ, CMD_ADDR};
              tx_bits <= 6'd32;
              rx_left <= 32'd8;
              state <= SELECT;
            end
            OP_CRC: begin
              tx <= {SPI_READ, CMD_ADDR};
              tx_bits <= 6'd32;
              rx_left <= CMD_LEN;
              state <= (CMD_LEN == 32'd0) ? MAKE_RESULT : SELECT;
            end
            OP_ERASE: begin
              addr_cur <= {CMD_ADDR[23:12], 12'h000};
              sectors_left <= 13'(({13'd0, CMD_ADDR[11:0]} + {1'b0, CMD_LEN[23:0]} + 25'hFFF) >> 12);
              if (!erase_allowed) begin
                err   <= ERR_PROTECTED;
                state <= MAKE_RESULT;
              end else begin
                state <= (CMD_LEN == 32'd0) ? MAKE_RESULT : ERASE_WREN;
              end
            end
            OP_PROGRAM: begin
              addr_cur <= CMD_ADDR;
              len_left <= CMD_LEN[23:0];
              prng <= (seed_mix == 32'd0) ? 32'd1 : seed_mix;
              if (!program_allowed) begin
                err   <= ERR_PROTECTED;
                state <= MAKE_RESULT;
              end else begin
                state <= (CMD_LEN[23:0] == 24'd0) ? MAKE_RESULT : PROGRAM_WREN;
              end
            end
            OP_IPROG, OP_WBSTAR: begin
              iprog_addr <= CMD_ADDR;
              iprog_timer <= CMD_LEN;
              iprog_reboot <= CMD_OP == OP_IPROG;
              iprog_start <= 1'b1;
              state <= MAKE_RESULT;
            end
            default: begin
              state <= MAKE_RESULT;
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
            if (rx_left != 32'd0) begin
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
            received <= {received[55:0], rx};
            crc <= crc32_update(crc, rx);
            rx_left <= rx_left - 32'd1;
            if (rx_left == 32'd1) begin
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
            mosi <= prng_next[7];
            wr_byte <= {prng_next[6:0], 1'b0};
            prng <= prng_next;
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
        tx <= {SPI_WREN, 24'd0};
        tx_bits <= 6'd8;
        ret <= ERASE_CMD;
        state <= SELECT;
      end
      ERASE_CMD: begin
        tx <= {SPI_SE4K, addr_cur};
        tx_bits <= 6'd32;
        addr_cur <= addr_cur + 24'h1000;
        sectors_left <= sectors_left - 13'd1;
        ret <= POLL;
        state <= SELECT;
      end
      PROGRAM_WREN: begin
        tx <= {SPI_WREN, 24'd0};
        tx_bits <= 6'd8;
        ret <= PROGRAM_CMD;
        state <= SELECT;
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
        rx_left <= 32'd1;
        ret <= POLL_CHECK;
        state <= SELECT;
      end
      POLL_CHECK: begin
        wait_cnt <= 16'd0;
        if (!received[0]) begin
          state <= NEXT;
        end else if (polls >= 16'(POLL_LIMIT)) begin
          err   <= ERR_TIMEOUT;
          state <= MAKE_RESULT;
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
        if ((op == OP_ERASE) && (sectors_left != 13'd0)) begin
          state <= ERASE_WREN;
        end else if ((op == OP_PROGRAM) && (len_left != 24'd0)) begin
          state <= PROGRAM_WREN;
        end else begin
          state <= MAKE_RESULT;
        end
      end
      MAKE_RESULT: begin
        if (op == OP_CRC) begin
          result <= {32'd0, ~crc};
        end else if ((op == OP_ERASE) || (op == OP_PROGRAM)) begin
          result <= {err, 48'd0, received[7:0]};
        end else begin
          result <= received;
        end
        state <= FINISH;
      end
      FINISH: begin
        busy  <= 1'b0;
        done  <= 1'b1;
        state <= IDLE;
      end
      default: begin
      end
    endcase
  end

endmodule
`default_nettype wire
