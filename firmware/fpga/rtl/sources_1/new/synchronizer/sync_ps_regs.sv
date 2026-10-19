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
    input wire settings::sync_ps_state_t STATE
);

  import params::*;

  localparam logic [15:0] KpDefault = 16'd64;
  localparam logic [15:0] KiDefault = 16'd1024;
  localparam logic [15:0] KfDefault = 16'd4096;

  localparam int SnapshotPeriodBits = 10;

  logic [ 1:0] ctl_reg = 2'b01;
  logic [15:0] kp_reg = KpDefault;
  logic [15:0] ki_reg = KiDefault;
  logic [15:0] kf_reg = KfDefault;
  logic [15:0] manual_rate_reg = '0;
  logic [ 3:0] write_count = '0;
  logic [ 3:0] write_count_gray = '0;
  logic [15:0] rdata = '0;

  logic [ 3:0] write_count_gray_sync;
  logic [ 3:0] write_count_gray_seen = '0;

  cdc_sync #(
      .WIDTH(4)
  ) write_count_cdc (
      .CLK(CLK),
      .D  (write_count_gray),
      .Q  (write_count_gray_sync)
  );

  logic update = 1'b0;
  logic enable = 1'b1;
  logic manual = 1'b0;
  logic [15:0] kp = KpDefault;
  logic [15:0] ki = KiDefault;
  logic [15:0] kf = KfDefault;
  logic signed [15:0] manual_rate = '0;

  assign SETTINGS.UPDATE = update;
  assign SETTINGS.ENABLE = enable;
  assign SETTINGS.MANUAL = manual;
  assign SETTINGS.KP = kp;
  assign SETTINGS.KI = ki;
  assign SETTINGS.KF = kf;
  assign SETTINGS.MANUAL_RATE = manual_rate;

  always_ff @(posedge BUS_CLK) begin
    if (WE & EN) begin
      case (ADDR)
        ADDR_SYNC_PS_CTL: ctl_reg <= DATA_IN[1:0];
        ADDR_SYNC_PS_KP: kp_reg <= DATA_IN;
        ADDR_SYNC_PS_KI: ki_reg <= DATA_IN;
        ADDR_SYNC_PS_KF: kf_reg <= DATA_IN;
        ADDR_SYNC_PS_MANUAL_RATE: manual_rate_reg <= DATA_IN;
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
      enable <= ctl_reg[0];
      manual <= ctl_reg[1];
      kp <= kp_reg;
      ki <= ki_reg;
      kf <= kf_reg;
      manual_rate <= manual_rate_reg;
      update <= 1'b1;
    end
  end

  logic [SnapshotPeriodBits-1:0] snapshot_cnt = '0;
  logic snapshot_toggle = 1'b0;
  logic fine_snap = 1'b0;
  logic [7:0] fine_exit_count_snap = '0;
  logic [15:0] rate_snap = '0;
  logic [15:0] integ_snap = '0;
  logic [15:0] pd_pos_snap = '0;
  logic [15:0] pd_neg_snap = '0;
  logic [15:0] edge_diff_snap = '0;
  logic [15:0] edge_count_snap = '0;

  always_ff @(posedge CLK) begin
    snapshot_cnt <= snapshot_cnt + 1'b1;
    if (snapshot_cnt == '0) begin
      fine_snap <= STATE.FINE;
      fine_exit_count_snap <= STATE.FINE_EXIT_COUNT;
      rate_snap <= STATE.RATE;
      integ_snap <= STATE.INTEG;
      pd_pos_snap <= STATE.PD_POS;
      pd_neg_snap <= STATE.PD_NEG;
      edge_diff_snap <= STATE.EDGE_DIFF;
      edge_count_snap <= STATE.EDGE_COUNT;
      snapshot_toggle <= ~snapshot_toggle;
    end
  end

  logic snapshot_taken;

  cdc_toggle snapshot_cdc (
      .CLK(BUS_CLK),
      .TOGGLE(snapshot_toggle),
      .PULSE(snapshot_taken)
  );

  logic fine_bus = 1'b0;
  logic [7:0] fine_exit_count_bus = '0;
  logic [15:0] rate_bus = '0;
  logic [15:0] integ_bus = '0;
  logic [15:0] pd_pos_bus = '0;
  logic [15:0] pd_neg_bus = '0;
  logic [15:0] edge_diff_bus = '0;
  logic [15:0] edge_count_bus = '0;

  always_ff @(posedge BUS_CLK) begin
    if (snapshot_taken) begin
      fine_bus <= fine_snap;
      fine_exit_count_bus <= fine_exit_count_snap;
      rate_bus <= rate_snap;
      integ_bus <= integ_snap;
      pd_pos_bus <= pd_pos_snap;
      pd_neg_bus <= pd_neg_snap;
      edge_diff_bus <= edge_diff_snap;
      edge_count_bus <= edge_count_snap;
    end
  end

  assign DATA_OUT = rdata;

  always_ff @(posedge BUS_CLK) begin
    if (EN) begin
      case (ADDR)
        ADDR_SYNC_PS_CTL: rdata <= {14'd0, ctl_reg};
        ADDR_SYNC_PS_KP: rdata <= kp_reg;
        ADDR_SYNC_PS_KI: rdata <= ki_reg;
        ADDR_SYNC_PS_KF: rdata <= kf_reg;
        ADDR_SYNC_PS_MANUAL_RATE: rdata <= manual_rate_reg;
        ADDR_SYNC_PS_STATE: rdata <= {fine_exit_count_bus, 7'd0, fine_bus};
        ADDR_SYNC_PS_RATE: rdata <= rate_bus;
        ADDR_SYNC_PS_INTEG: rdata <= integ_bus;
        ADDR_SYNC_PS_PD_POS: rdata <= pd_pos_bus;
        ADDR_SYNC_PS_PD_NEG: rdata <= pd_neg_bus;
        ADDR_SYNC_PS_EDGE_DIFF: rdata <= edge_diff_bus;
        ADDR_SYNC_PS_EDGE_COUNT: rdata <= edge_count_bus;
        default: rdata <= 16'd0;
      endcase
    end
  end

endmodule
`default_nettype wire
