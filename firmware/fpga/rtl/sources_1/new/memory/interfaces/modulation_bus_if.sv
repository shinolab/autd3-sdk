`timescale 1ns / 1ps
interface modulation_bus_if ();

  logic [15:0] IDX;
  logic [7:0] VALUE;
  logic BANK;

  modport in_port(input IDX, output VALUE, input BANK);
  modport out_port(output IDX, input VALUE, output BANK);

endinterface
