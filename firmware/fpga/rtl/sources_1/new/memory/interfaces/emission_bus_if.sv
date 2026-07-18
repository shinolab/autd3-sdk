`timescale 1ns / 1ps
`default_nettype none
interface emission_bus_if ();

  logic [15:0] ADDR;
  logic [63:0] VALUE;
  logic MODE;
  logic BANK;

  logic [9:0] RAW_IDX;
  logic [7:0] RAW_ADDR;
  logic [15:0] FOCUS_IDX;

  logic RD_EN;
  logic RAW_RD_EN;
  logic FOCUS_RD_EN;

  assign ADDR  = MODE ? {RAW_IDX, RAW_ADDR[7:2]} : FOCUS_IDX;
  assign RD_EN = MODE ? RAW_RD_EN : FOCUS_RD_EN;

  modport in_port(input ADDR, output VALUE, input BANK, input RD_EN);
  modport emission_port(output MODE, output BANK);
  modport out_raw_port(output RAW_IDX, output RAW_ADDR, input VALUE, output RAW_RD_EN);
  modport out_focus_port(output FOCUS_IDX, input VALUE, output FOCUS_RD_EN);

endinterface
`default_nettype wire
