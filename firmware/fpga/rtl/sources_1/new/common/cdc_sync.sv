`timescale 1ns / 1ps
`default_nettype none
module cdc_sync #(
    parameter int WIDTH = 1
) (
    input wire CLK,
    input wire [WIDTH-1:0] D,
    output wire [WIDTH-1:0] Q
);

  (* ASYNC_REG = "TRUE" *)logic [WIDTH-1:0] meta = '0;
  (* ASYNC_REG = "TRUE" *)logic [WIDTH-1:0] sync = '0;

  assign Q = sync;

  always_ff @(posedge CLK) begin
    meta <= D;
    sync <= meta;
  end

endmodule
`default_nettype wire
