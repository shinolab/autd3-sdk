`timescale 1ns / 1ps
`default_nettype none
interface modulation_bus_if ();

  logic [15:0] IDX;
  logic [7:0] VALUE;
  logic BANK;
  logic RD_EN;

  modport in_port(input IDX, output VALUE, input BANK, input RD_EN);
  modport out_port(output IDX, input VALUE, output BANK, output RD_EN);

endinterface
`default_nettype wire
