`timescale 1ns / 1ps
`default_nettype none
interface flash_bus_if ();

  logic CMD_VALID;
  logic CMD_ACCEPT;
  logic [7:0] CMD_OP;
  logic [23:0] CMD_ADDR;
  logic [23:0] CMD_LEN;
  logic [9:0] BUF_IDX;
  logic [7:0] BUF_BYTE;
  logic DONE;
  logic [7:0] ERR;
  logic [31:0] RESULT;
  logic [31:0] USR_ACCESS;

  modport host_port(
      output CMD_VALID,
      input CMD_ACCEPT,
      output CMD_OP,
      output CMD_ADDR,
      output CMD_LEN,
      input BUF_IDX,
      output BUF_BYTE,
      input DONE,
      input ERR,
      input RESULT,
      input USR_ACCESS
  );

  modport core_port(
      input CMD_VALID,
      output CMD_ACCEPT,
      input CMD_OP,
      input CMD_ADDR,
      input CMD_LEN,
      output BUF_IDX,
      input BUF_BYTE,
      output DONE,
      output ERR,
      output RESULT,
      output USR_ACCESS
  );

endinterface
`default_nettype wire
