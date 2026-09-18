`timescale 1ns / 1ps
`default_nettype none
module flash_iprog (
    input wire CLK,
    input wire START,
    output var ICAP_CSIB,
    output var [31:0] ICAP_I
);

  localparam int NumWords = 8;
  localparam logic [31:0] Sequence[NumWords] = '{
      32'hFFFFFFFF,
      32'hAA995566,
      32'h20000000,
      32'h30020001,
      32'h00000000,
      32'h30008001,
      32'h0000000F,
      32'h20000000
  };

  logic [3:0] idx = 4'd0;
  logic active = 1'b0;
  logic csib = 1'b1;
  logic [31:0] data = 32'hFFFFFFFF;

  assign ICAP_CSIB = csib;
  assign ICAP_I = data;

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
      idx  <= 4'd0;
      if (START) begin
        active <= 1'b1;
      end
    end else begin
      csib <= 1'b0;
      data <= bit_swap(Sequence[idx]);
      idx  <= idx + 4'd1;
      if (idx == 4'(NumWords - 1)) begin
        active <= 1'b0;
      end
    end
  end

endmodule
`default_nettype wire
