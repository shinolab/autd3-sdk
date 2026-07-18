`timescale 1ns / 1ps
`default_nettype none
interface output_mask_bus_if ();

  logic BANK;
  logic [255:0] VALUE;
  logic RD_EN;

  modport in_port(input BANK, output VALUE, input RD_EN);
  modport out_port(output BANK, input VALUE, output RD_EN);

endinterface
`default_nettype wire
