`timescale 1ns / 1ps
`default_nettype none
module cdc_toggle (
    input  wire CLK,
    input  wire TOGGLE,
    output wire PULSE
);

  logic toggle_sync;
  logic toggle_seen = 1'b0;

  cdc_sync cdc_sync (
      .CLK(CLK),
      .D  (TOGGLE),
      .Q  (toggle_sync)
  );

  assign PULSE = toggle_seen != toggle_sync;

  always_ff @(posedge CLK) toggle_seen <= toggle_sync;

endmodule
`default_nettype wire
