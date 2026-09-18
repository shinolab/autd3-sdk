`timescale 1ns / 1ps
`default_nettype none
module top (
    input wire [16:1] CPU_ADDR,
    inout tri [15:0] CPU_DATA,
    input wire CPU_CKIO,
    input wire CPU_CS1_N,
    input wire RESET_N,
    input wire CPU_WE0_N,
    input wire CPU_RD_N,
    input wire CPU_RDWR,
    input wire MRCC_25P6M,
    input wire CAT_SYNC0,
    output wire FORCE_FAN,
    input wire THERMO,
    output wire [252:1] XDCR_OUT,
    input wire GPIO_IN[4],
    output wire GPIO_OUT[4],
    output wire FLASH_CS_N,
    output wire FLASH_MOSI,
    input wire FLASH_MISO,
    output wire FLASH_WP_N,
    output wire FLASH_HOLD_N
);

  logic reset;

  logic PWM_OUT[params::NumTransducers];

  assign reset = ~RESET_N;

  wire flash_eos;
  wire flash_sck;
  wire [31:0] usr_access;
  wire icap_clk;
  wire icap_csib;
  wire [31:0] icap_i;

  assign FLASH_WP_N   = 1'b1;
  assign FLASH_HOLD_N = 1'b1;

  STARTUPE2 #(
      .PROG_USR("FALSE"),
      .SIM_CCLK_FREQ(0.0)
  ) startup (
      .CFGCLK(),
      .CFGMCLK(),
      .EOS(flash_eos),
      .PREQ(),
      .CLK(1'b0),
      .GSR(1'b0),
      .GTS(1'b0),
      .KEYCLEARB(1'b1),
      .PACK(1'b0),
      .USRCCLKO(flash_sck),
      .USRCCLKTS(1'b0),
      .USRDONEO(1'b1),
      .USRDONETS(1'b1)
  );

  ICAPE2 #(
      .DEVICE_ID(32'h03636093),
      .ICAP_WIDTH("X32"),
      .SIM_CFG_FILE_NAME("NONE")
  ) icap (
      .O(),
      .CLK(icap_clk),
      .CSIB(icap_csib),
      .I(icap_i),
      .RDWRB(1'b0)
  );

  USR_ACCESSE2 usr_access_reg (
      .CFGCLK(),
      .DATA(usr_access),
      .DATAVALID()
  );

  for (genvar i = 0; i < params::NumTransducers; i++) begin : gen_output
    assign XDCR_OUT[cvt_uid(i)+1] = PWM_OUT[i];
  end

  assign XDCR_OUT[20] = 1'b0;
  assign XDCR_OUT[21] = 1'b0;
  assign XDCR_OUT[35] = 1'b0;

  memory_bus_if memory_bus ();
  assign memory_bus.BUS_CLK = CPU_CKIO;
  assign memory_bus.EN = ~CPU_CS1_N;
  assign memory_bus.RD = ~CPU_RD_N;
  assign memory_bus.RDWR = CPU_RDWR;
  assign memory_bus.WE = ~CPU_WE0_N;
  assign memory_bus.BRAM_SELECT = CPU_ADDR[16:15];
  assign memory_bus.BRAM_ADDR = CPU_ADDR[14:1];
  assign memory_bus.CPU_DATA = CPU_DATA;

  main #(
      .DEPTH(params::NumTransducers)
  ) main (
      .MRCC_25P6M(MRCC_25P6M),
      .RESET(reset),
      .CAT_SYNC0(CAT_SYNC0),
      .MEM_BUS(memory_bus.bram_port),
      .THERMO(THERMO),
      .FORCE_FAN(FORCE_FAN),
      .PWM_OUT(PWM_OUT),
      .GPIO_IN_HARD(GPIO_IN),
      .GPIO_OUT(GPIO_OUT),
      .FLASH_EOS(flash_eos),
      .USR_ACCESS(usr_access),
      .FLASH_SCK(flash_sck),
      .FLASH_CS_N(FLASH_CS_N),
      .FLASH_MOSI(FLASH_MOSI),
      .FLASH_MISO(FLASH_MISO),
      .ICAP_CLK(icap_clk),
      .ICAP_CSIB(icap_csib),
      .ICAP_I(icap_i)
  );

  function automatic [7:0] cvt_uid(input logic [7:0] idx);
    if (idx < 8'd19) begin
      cvt_uid = idx;
    end else if (idx < 8'd32) begin
      cvt_uid = idx + 2;
    end else begin
      cvt_uid = idx + 3;
    end
  endfunction

endmodule
`default_nettype wire
