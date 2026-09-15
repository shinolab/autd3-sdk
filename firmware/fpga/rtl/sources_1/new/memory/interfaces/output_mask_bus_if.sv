`timescale 1ns / 1ps
`default_nettype none
interface output_mask_bus_if ();

  logic [255:0] VALUE;
  logic RD_EN;

  modport in_port(output VALUE, input RD_EN);
  modport out_port(input VALUE, output RD_EN);

endinterface
`default_nettype wire
