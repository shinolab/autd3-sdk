`timescale 1ns / 1ps
`default_nettype none
module sync_ps_regs (
    input wire BUS_CLK,
    input wire CLK,
    input wire EN,
    input wire WE,
    input wire [7:0] ADDR,
    input wire [15:0] DATA_IN,
    output var [15:0] DATA_OUT,
    output var settings::sync_ps_settings_t SETTINGS
);

  import params::*;

  localparam logic [15:0] KpDefault = 16'd64;
  localparam logic [15:0] KiDefault = 16'd1024;
  localparam logic [15:0] KfDefault = 16'd4096;

  logic ctl_reg = 1'b1;
  logic [15:0] kp_reg = KpDefault;
  logic [15:0] ki_reg = KiDefault;
  logic [15:0] kf_reg = KfDefault;
  logic [3:0] write_count = '0;
  logic [3:0] write_count_gray = '0;
  logic [15:0] rdata = '0;

  logic [3:0] write_count_gray_sync;
  logic [3:0] write_count_gray_seen = '0;

  cdc_sync #(
      .WIDTH(4)
  ) write_count_cdc (
      .CLK(CLK),
      .D  (write_count_gray),
      .Q  (write_count_gray_sync)
  );

  logic update = 1'b0;
  logic enable = 1'b1;
  logic [15:0] kp = KpDefault;
  logic [15:0] ki = KiDefault;
  logic [15:0] kf = KfDefault;

  assign SETTINGS.UPDATE = update;
  assign SETTINGS.ENABLE = enable;
  assign SETTINGS.KP = kp;
  assign SETTINGS.KI = ki;
  assign SETTINGS.KF = kf;

  always_ff @(posedge BUS_CLK) begin
    if (WE & EN) begin
      case (ADDR)
        ADDR_SYNC_PS_CTL: ctl_reg <= DATA_IN[0];
        ADDR_SYNC_PS_KP:  kp_reg <= DATA_IN;
        ADDR_SYNC_PS_KI:  ki_reg <= DATA_IN;
        ADDR_SYNC_PS_KF:  kf_reg <= DATA_IN;
        default: begin
        end
      endcase
      write_count <= write_count + 4'd1;
    end
    write_count_gray <= write_count ^ {1'b0, write_count[3:1]};
  end

  always_ff @(posedge CLK) begin
    update <= 1'b0;
    write_count_gray_seen <= write_count_gray_sync;
    if (write_count_gray_sync != write_count_gray_seen) begin
      enable <= ctl_reg;
      kp <= kp_reg;
      ki <= ki_reg;
      kf <= kf_reg;
      update <= 1'b1;
    end
  end

  assign DATA_OUT = rdata;

  always_ff @(posedge BUS_CLK) begin
    if (EN) begin
      case (ADDR)
        ADDR_SYNC_PS_CTL: rdata <= {15'd0, ctl_reg};
        ADDR_SYNC_PS_KP: rdata <= kp_reg;
        ADDR_SYNC_PS_KI: rdata <= ki_reg;
        ADDR_SYNC_PS_KF: rdata <= kf_reg;
        default: rdata <= 16'd0;
      endcase
    end
  end

endmodule
`default_nettype wire
