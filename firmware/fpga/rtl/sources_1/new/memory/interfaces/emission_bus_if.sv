`timescale 1ns / 1ps
interface emission_bus_if ();

  logic [15:0] ADDR;
  logic [63:0] VALUE;
  logic MODE;
  logic BANK;

  logic [9:0] RAW_IDX;
  logic [7:0] RAW_ADDR;
  logic [15:0] FOCUS_IDX;

  assign ADDR = MODE ? {RAW_IDX, RAW_ADDR[7:2]} : FOCUS_IDX;

  modport in_port(input ADDR, output VALUE, input BANK);
  modport emission_port(output MODE, output BANK);
  modport out_raw_port(output RAW_IDX, output RAW_ADDR, input VALUE);
  modport out_focus_port(output FOCUS_IDX, input VALUE);

endinterface
