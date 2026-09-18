`timescale 1ns / 1ps
`default_nettype none
module flash_probe_top (
    input  wire MRCC_25P6M,
    output wire FLASH_CS_N,
    output wire FLASH_MOSI,
    input  wire FLASH_MISO,
    output wire FLASH_WP_N,
    output wire FLASH_HOLD_N
);

  localparam int DR_BITS = 80;
  localparam logic [15:0] MAGIC = 16'hA55A;
  localparam logic [5:0] SIGNATURE = 6'b101101;

  wire clk;
  wire eos;
  wire sck;
  wire tck;
  wire tdi;
  wire tdo;
  wire sel;
  wire capture;
  wire shift;
  wire update;

  assign clk = MRCC_25P6M;
  assign FLASH_WP_N = 1'b1;
  assign FLASH_HOLD_N = 1'b1;

  STARTUPE2 #(
      .PROG_USR("FALSE"),
      .SIM_CCLK_FREQ(0.0)
  ) startup (
      .CFGCLK(),
      .CFGMCLK(),
      .EOS(eos),
      .PREQ(),
      .CLK(1'b0),
      .GSR(1'b0),
      .GTS(1'b0),
      .KEYCLEARB(1'b1),
      .PACK(1'b0),
      .USRCCLKO(sck),
      .USRCCLKTS(1'b0),
      .USRDONEO(1'b1),
      .USRDONETS(1'b1)
  );

  BSCANE2 #(
      .JTAG_CHAIN(1)
  ) bscan (
      .CAPTURE(capture),
      .DRCK(),
      .RESET(),
      .RUNTEST(),
      .SEL(sel),
      .SHIFT(shift),
      .TCK(tck),
      .TDI(tdi),
      .TMS(),
      .UPDATE(update),
      .TDO(tdo)
  );

  logic [DR_BITS-1:0] dr = '0;
  logic [DR_BITS-1:0] cmd = '0;
  logic cmd_toggle = 1'b0;

  logic [63:0] result;
  logic [7:0] result_op;
  logic busy;
  logic done;

  assign tdo = dr[0];

  always_ff @(posedge tck) begin
    if (sel && capture) begin
      dr <= {SIGNATURE, done, busy, result_op, result};
    end else if (sel && shift) begin
      dr <= {tdi, dr[DR_BITS-1:1]};
    end
    if (sel && update && (dr[79:64] == MAGIC)) begin
      cmd <= dr;
      cmd_toggle <= ~cmd_toggle;
    end
  end

  (* ASYNC_REG = "TRUE" *) logic [1:0] eos_sync = 2'b00;
  (* ASYNC_REG = "TRUE" *) logic [1:0] toggle_sync = 2'b00;
  logic toggle_seen = 1'b0;
  logic cmd_valid = 1'b0;
  logic [DR_BITS-1:0] cmd_clk = '0;

  always_ff @(posedge clk) begin
    eos_sync <= {eos_sync[0], eos};
    toggle_sync <= {toggle_sync[0], cmd_toggle};
    toggle_seen <= toggle_sync[1];
    cmd_valid <= toggle_seen != toggle_sync[1];
    if (toggle_seen != toggle_sync[1]) begin
      cmd_clk <= cmd;
    end
  end

  wire iprog_start;
  wire iprog_reboot;
  wire [23:0] iprog_addr;
  wire [31:0] iprog_timer;
  wire icap_csib;
  wire icap_rdwrb;
  wire [31:0] icap_i;

  flash_probe_core core (
      .CLK(clk),
      .START(eos_sync[1]),
      .CMD_VALID(cmd_valid),
      .CMD_OP(cmd_clk[7:0]),
      .CMD_ADDR(cmd_clk[31:8]),
      .CMD_LEN(cmd_clk[63:32]),
      .RESULT(result),
      .RESULT_OP(result_op),
      .BUSY(busy),
      .DONE(done),
      .SCK(sck),
      .CS_N(FLASH_CS_N),
      .MOSI(FLASH_MOSI),
      .MISO(FLASH_MISO),
      .IPROG_START(iprog_start),
      .IPROG_REBOOT(iprog_reboot),
      .IPROG_ADDR(iprog_addr),
      .IPROG_TIMER(iprog_timer)
  );

  flash_probe_iprog iprog (
      .CLK(clk),
      .START(iprog_start),
      .REBOOT(iprog_reboot),
      .ADDR(iprog_addr),
      .TIMER(iprog_timer),
      .ICAP_CSIB(icap_csib),
      .ICAP_RDWRB(icap_rdwrb),
      .ICAP_I(icap_i)
  );

  ICAPE2 #(
      .DEVICE_ID(32'h03636093),
      .ICAP_WIDTH("X32"),
      .SIM_CFG_FILE_NAME("NONE")
  ) icap (
      .O(),
      .CLK(clk),
      .CSIB(icap_csib),
      .I(icap_i),
      .RDWRB(icap_rdwrb)
  );

endmodule
`default_nettype wire
