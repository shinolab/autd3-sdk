`timescale 1ns / 1ps
interface output_mask_bus_if ();

  logic BANK;
  logic [255:0] VALUE;

  modport in_port(input BANK, output VALUE);
  modport out_port(output BANK, input VALUE);

endinterface
