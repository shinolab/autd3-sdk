`timescale 1ns / 1ps
`default_nettype none
interface phase_corr_bus_if ();

  logic [7:0] IDX;
  logic [7:0] VALUE;
  logic RD_EN;

  modport in_port(input IDX, output VALUE, input RD_EN);
  modport out_port(output IDX, input VALUE, output RD_EN);

endinterface
`default_nettype wire
