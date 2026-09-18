`timescale 1ns / 1ps
`default_nettype none
module flash_host (
    input wire BUS_CLK,
    input wire CLK,
    input wire REG_EN,
    input wire BUF_EN,
    input wire WE,
    input wire [8:0] ADDR,
    input wire [15:0] DATA_IN,
    output var [15:0] DATA_OUT,
    flash_bus_if.host_port FLASH_BUS
);

  import params::*;

  localparam int BufWords = FlashBufBytes / 2;

  logic [7:0] op_reg = 8'd0;
  logic [23:0] addr_reg = 24'd0;
  logic [23:0] len_reg = 24'd0;
  logic req = 1'b0;
  logic [2:0] we_edge = 3'b000;
  logic [15:0] rdata = 16'd0;

  (* ASYNC_REG = "TRUE" *) logic [1:0] ack_sync = 2'b00;
  logic ack = 1'b0;
  logic busy;

  assign busy = req != ack_sync[1];
  assign DATA_OUT = rdata;

  always_ff @(posedge BUS_CLK) begin
    ack_sync <= {ack_sync[0], ack};
    we_edge  <= {we_edge[1:0], WE & REG_EN};
    if (we_edge == 3'b011) begin
      case (ADDR[7:0])
        ADDR_FLASH_CMD: begin
          if (!busy) begin
            op_reg <= DATA_IN[7:0];
            req <= ~req;
          end
        end
        ADDR_FLASH_ADDR_0: addr_reg[15:0] <= DATA_IN;
        ADDR_FLASH_ADDR_1: addr_reg[23:16] <= DATA_IN[7:0];
        ADDR_FLASH_LEN_0:  len_reg[15:0] <= DATA_IN;
        ADDR_FLASH_LEN_1:  len_reg[23:16] <= DATA_IN[7:0];
        default: begin
        end
      endcase
    end
    if (REG_EN) begin
      case (ADDR[7:0])
        ADDR_FLASH_CMD: rdata <= {8'd0, op_reg};
        ADDR_FLASH_ADDR_0: rdata <= addr_reg[15:0];
        ADDR_FLASH_ADDR_1: rdata <= {8'd0, addr_reg[23:16]};
        ADDR_FLASH_LEN_0: rdata <= len_reg[15:0];
        ADDR_FLASH_LEN_1: rdata <= {8'd0, len_reg[23:16]};
        ADDR_FLASH_STATUS: rdata <= {FLASH_BUS.ERR, 7'd0, busy};
        ADDR_FLASH_RESULT_0: rdata <= FLASH_BUS.RESULT[15:0];
        ADDR_FLASH_RESULT_1: rdata <= FLASH_BUS.RESULT[31:16];
        ADDR_FLASH_USR_ACCESS_0: rdata <= FLASH_BUS.USR_ACCESS[15:0];
        ADDR_FLASH_USR_ACCESS_1: rdata <= FLASH_BUS.USR_ACCESS[31:16];
        default: rdata <= 16'd0;
      endcase
    end
  end

  (* ram_style = "block" *) logic [15:0] buf_mem[BufWords];
  logic [15:0] buf_word = 16'd0;
  logic buf_msb = 1'b0;

  always_ff @(posedge BUS_CLK) begin
    if (BUF_EN & WE) begin
      buf_mem[ADDR] <= DATA_IN;
    end
  end

  always_ff @(posedge CLK) begin
    buf_word <= buf_mem[FLASH_BUS.BUF_IDX[9:1]];
    buf_msb  <= FLASH_BUS.BUF_IDX[0];
  end

  assign FLASH_BUS.BUF_BYTE = buf_msb ? buf_word[15:8] : buf_word[7:0];

  (* ASYNC_REG = "TRUE" *) logic [1:0] req_sync = 2'b00;
  logic req_seen = 1'b0;
  logic pending = 1'b0;
  logic [7:0] cmd_op = 8'd0;
  logic [23:0] cmd_addr = 24'd0;
  logic [23:0] cmd_len = 24'd0;

  assign FLASH_BUS.CMD_VALID = pending;
  assign FLASH_BUS.CMD_OP = cmd_op;
  assign FLASH_BUS.CMD_ADDR = cmd_addr;
  assign FLASH_BUS.CMD_LEN = cmd_len;

  always_ff @(posedge CLK) begin
    req_sync <= {req_sync[0], req};
    req_seen <= req_sync[1];
    if (req_seen != req_sync[1]) begin
      pending  <= 1'b1;
      cmd_op   <= op_reg;
      cmd_addr <= addr_reg;
      cmd_len  <= len_reg;
    end else if (FLASH_BUS.CMD_ACCEPT) begin
      pending <= 1'b0;
    end
    if (FLASH_BUS.DONE) begin
      ack <= ~ack;
    end
  end

endmodule
`default_nettype wire
