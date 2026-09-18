`timescale 1ns / 1ps
`default_nettype none
module flash #(
    parameter int POLL_INTERVAL = 20480,
    parameter int POLL_LIMIT = 5000
) (
    input wire CLK,
    input wire LOCKED,
    input wire EOS,
    input wire [31:0] USR_ACCESS,
    flash_bus_if.core_port FLASH_BUS,
    output wire SCK,
    output wire CS_N,
    output wire MOSI,
    input wire MISO,
    output wire ICAP_CSIB,
    output wire [31:0] ICAP_I
);

  (* ASYNC_REG = "TRUE" *) logic [1:0] eos_sync = 2'b00;
  logic reboot;
  logic start;

  always_ff @(posedge CLK) begin
    eos_sync <= {eos_sync[0], EOS};
    start <= eos_sync[1] & LOCKED;
  end

  assign FLASH_BUS.USR_ACCESS = USR_ACCESS;

  flash_spi #(
      .POLL_INTERVAL(POLL_INTERVAL),
      .POLL_LIMIT(POLL_LIMIT)
  ) flash_spi (
      .CLK(CLK),
      .START(start),
      .CMD_VALID(FLASH_BUS.CMD_VALID),
      .CMD_ACCEPT(FLASH_BUS.CMD_ACCEPT),
      .CMD_OP(FLASH_BUS.CMD_OP),
      .CMD_ADDR(FLASH_BUS.CMD_ADDR),
      .CMD_LEN(FLASH_BUS.CMD_LEN),
      .BUF_IDX(FLASH_BUS.BUF_IDX),
      .BUF_BYTE(FLASH_BUS.BUF_BYTE),
      .DONE(FLASH_BUS.DONE),
      .ERR(FLASH_BUS.ERR),
      .RESULT(FLASH_BUS.RESULT),
      .REBOOT(reboot),
      .SCK(SCK),
      .CS_N(CS_N),
      .MOSI(MOSI),
      .MISO(MISO)
  );

  flash_iprog flash_iprog (
      .CLK(CLK),
      .START(reboot & LOCKED),
      .ICAP_CSIB(ICAP_CSIB),
      .ICAP_I(ICAP_I)
  );

endmodule
`default_nettype wire
