`timescale 1ns / 1ps
`default_nettype none
interface memory_bus_if ();

  logic BUS_CLK;
  logic CS_N;
  logic RD_N;
  logic WE_N;
  logic RDWR;
  logic [1:0] BRAM_SELECT;
  logic [13:0] BRAM_ADDR;
  logic [15:0] CPU_DATA;
  logic [15:0] DATA_IN;
  logic [15:0] DATA_OUT;

  assign CPU_DATA = (~CS_N & ~RD_N & RDWR) ? DATA_OUT : 16'bzzzzzzzzzzzzzzzz;
  assign DATA_IN  = CPU_DATA;

  modport bram_port(input BUS_CLK, input CS_N, input WE_N, input BRAM_SELECT, input BRAM_ADDR, input DATA_IN, output DATA_OUT);

endinterface
`default_nettype wire
