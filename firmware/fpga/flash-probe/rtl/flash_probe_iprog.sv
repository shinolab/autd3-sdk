`timescale 1ns / 1ps
`default_nettype none
module flash_probe_iprog (
    input wire CLK,
    input wire START,
    input wire REBOOT,
    input wire [23:0] ADDR,
    input wire [31:0] TIMER,
    output var ICAP_CSIB,
    output var ICAP_RDWRB,
    output var [31:0] ICAP_I
);

  localparam logic [31:0] DUMMY = 32'hFFFFFFFF;
  localparam logic [31:0] SYNC = 32'hAA995566;
  localparam logic [31:0] NOOP = 32'h20000000;
  localparam logic [31:0] WRITE_TIMER = 32'h30022001;
  localparam logic [31:0] WRITE_WBSTAR = 32'h30020001;
  localparam logic [31:0] WRITE_CMD = 32'h30008001;
  localparam logic [31:0] CMD_IPROG = 32'h0000000F;
  localparam logic [31:0] CMD_DESYNC = 32'h0000000D;

  localparam int MAX_WORDS = 10;

  logic [31:0] words[MAX_WORDS];
  logic [3:0] count = 4'd0;
  logic [3:0] idx = 4'd0;
  logic active = 1'b0;
  logic csib = 1'b1;
  logic [31:0] data = 32'd0;

  logic [31:0] cmd_body;

  assign ICAP_CSIB = csib;
  assign ICAP_RDWRB = 1'b0;
  assign ICAP_I = data;

  assign cmd_body = REBOOT ? CMD_IPROG : CMD_DESYNC;

  function automatic logic [31:0] bit_swap(input logic [31:0] w);
    logic [31:0] s;
    for (int i = 0; i < 4; i++) begin
      for (int b = 0; b < 8; b++) begin
        s[i*8+b] = w[i*8+7-b];
      end
    end
    return s;
  endfunction

  always_ff @(posedge CLK) begin
    if (!active) begin
      csib <= 1'b1;
      if (START) begin
        words[0] <= DUMMY;
        words[1] <= SYNC;
        words[2] <= NOOP;
        if (TIMER != 32'd0) begin
          words[3] <= WRITE_TIMER;
          words[4] <= TIMER;
          words[5] <= WRITE_WBSTAR;
          words[6] <= {8'd0, ADDR};
          words[7] <= WRITE_CMD;
          words[8] <= cmd_body;
          words[9] <= NOOP;
          count <= 4'd10;
        end else begin
          words[3] <= WRITE_WBSTAR;
          words[4] <= {8'd0, ADDR};
          words[5] <= WRITE_CMD;
          words[6] <= cmd_body;
          words[7] <= NOOP;
          words[8] <= NOOP;
          words[9] <= NOOP;
          count <= 4'd8;
        end
        idx <= 4'd0;
        active <= 1'b1;
      end
    end else begin
      csib <= 1'b0;
      data <= bit_swap(words[idx]);
      idx  <= idx + 4'd1;
      if (idx == count - 4'd1) begin
        active <= 1'b0;
      end
    end
  end

endmodule
`default_nettype wire
