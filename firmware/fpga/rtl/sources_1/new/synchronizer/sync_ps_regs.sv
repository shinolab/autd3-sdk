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
    output var settings::sync_ps_settings_t SETTINGS,
    input wire settings::sync_ps_status_t STATUS
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
  logic [1:0] stats_cmd = '0;
  logic stats_req = 1'b0;
  logic stats_ack_sync;

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
        ADDR_SYNC_PS_STATS_CTL: begin
          if (stats_req == stats_ack_sync) begin
            stats_cmd <= DATA_IN[1:0];
            stats_req <= ~stats_req;
          end
        end
        default: begin
        end
      endcase
      if (ADDR <= ADDR_SYNC_PS_KF) write_count <= write_count + 4'd1;
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

  logic stats_req_pulse;
  logic stats_ack = 1'b0;
  logic signed [15:0] diff_min;
  logic signed [15:0] diff_max;
  logic [15:0] diff_outliers;
  logic signed [15:0] rate_min;
  logic signed [15:0] rate_max;
  logic [15:0] fine_exits;

  cdc_toggle stats_req_cdc (
      .CLK(CLK),
      .TOGGLE(stats_req),
      .PULSE(stats_req_pulse)
  );

  always_ff @(posedge CLK) if (stats_req_pulse) stats_ack <= ~stats_ack;

  cdc_sync stats_ack_cdc (
      .CLK(BUS_CLK),
      .D  (stats_ack),
      .Q  (stats_ack_sync)
  );

  sync_ps_stats sync_ps_stats (
      .CLK(CLK),
      .STATUS(STATUS),
      .SNAPSHOT(stats_req_pulse & stats_cmd[0]),
      .CLEAR(stats_req_pulse & stats_cmd[1]),
      .DIFF_MIN(diff_min),
      .DIFF_MAX(diff_max),
      .DIFF_OUTLIERS(diff_outliers),
      .RATE_MIN(rate_min),
      .RATE_MAX(rate_max),
      .FINE_EXITS(fine_exits)
  );

  assign DATA_OUT = rdata;

  always_ff @(posedge BUS_CLK) begin
    if (EN) begin
      case (ADDR)
        ADDR_SYNC_PS_CTL: rdata <= {15'd0, ctl_reg};
        ADDR_SYNC_PS_KP: rdata <= kp_reg;
        ADDR_SYNC_PS_KI: rdata <= ki_reg;
        ADDR_SYNC_PS_KF: rdata <= kf_reg;
        ADDR_SYNC_PS_STATS_CTL: rdata <= {15'd0, stats_req ^ stats_ack_sync};
        ADDR_SYNC_PS_DIFF_MIN: rdata <= diff_min;
        ADDR_SYNC_PS_DIFF_MAX: rdata <= diff_max;
        ADDR_SYNC_PS_DIFF_OUTLIERS: rdata <= diff_outliers;
        ADDR_SYNC_PS_RATE_MIN: rdata <= rate_min;
        ADDR_SYNC_PS_RATE_MAX: rdata <= rate_max;
        ADDR_SYNC_PS_FINE_EXITS: rdata <= fine_exits;
        default: rdata <= 16'd0;
      endcase
    end
  end

endmodule
`default_nettype wire
