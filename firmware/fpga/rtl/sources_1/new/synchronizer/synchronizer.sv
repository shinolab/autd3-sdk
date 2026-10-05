`timescale 1ns / 1ps
`default_nettype none
module synchronizer (
    input wire CLK,
    input wire settings::sync_settings_t SYNC_SETTINGS,
    input wire SYNC_IN,
    output var [56:0] SYS_TIME,
    output var SYNC,
    output var SKIP_ONE_ASSERT,
    output var signed [13:0] SYNC_TIME_DIFF,
    output var [7:0] SYNC_RESYNC_COUNT
);

  localparam int CycleTicks = params::SyncCycleTicks;
  localparam int HalfCycleTicks = CycleTicks / 2;
  localparam int PhaseWidth = $clog2(CycleTicks + 2);

  localparam int AdjustCntBase = 3125;
  localparam int AdjustCntRange = 4096;
  localparam int AdjustCntOffset = AdjustCntBase - AdjustCntRange / 2;

  localparam int DiffMax = 8191;

  (* ASYNC_REG = "true" *) logic [2:0] sync_tri = '0;
  logic sync;
  assign sync = sync_tri[2:1] == 2'b01;
  assign SYNC = sync;

  logic [56:0] sync_time = '0;
  logic set = 1'b0;

  logic [56:0] sys_time = '0;
  logic [PhaseWidth-1:0] phase = '0;
  logic signed [13:0] sync_time_diff = '0;
  logic skip_one_assert = 1'b0;
  logic [7:0] resync_count = '0;
  logic locked = 1'b0;

  logic [31:0] xor_x = 32'd123456789;
  logic [31:0] xor_y = 32'd362436069;
  logic [31:0] xor_z = 32'd521288629;
  logic [31:0] xor_w = 32'd88675123;
  logic [31:0] xor_t;

  logic [12:0] adjust_cnt = '0;
  logic [12:0] adjust_cnt_cyc = 13'(AdjustCntBase);

  assign SYS_TIME = sys_time;
  assign SKIP_ONE_ASSERT = skip_one_assert;
  assign SYNC_TIME_DIFF = sync_time_diff;
  assign SYNC_RESYNC_COUNT = resync_count;

  logic signed [PhaseWidth:0] edge_diff;
  assign edge_diff = (phase >= PhaseWidth'(HalfCycleTicks)) ? $signed({1'b0, PhaseWidth'(CycleTicks) - phase}) : -$signed({1'b0, phase});

  logic edge_overflow;
  assign edge_overflow = (edge_diff > DiffMax) || (edge_diff < -DiffMax);

  logic [1:0] step;
  always_comb begin
    if (sync) step = 2'd1;
    else if ((adjust_cnt != '0) || (sync_time_diff == '0)) step = 2'd1;
    else if (sync_time_diff < 14'sd0) step = 2'd0;
    else step = 2'd2;
  end

  logic [PhaseWidth-1:0] phase_next;
  assign phase_next = (phase + PhaseWidth'(step) >= PhaseWidth'(CycleTicks)) ? phase + PhaseWidth'(step) - PhaseWidth'(CycleTicks) : phase + PhaseWidth'(step);

  always_ff @(posedge CLK) begin
    if (SYNC_SETTINGS.UPDATE) begin
      set <= 1'b1;
      sync_time <= SYNC_SETTINGS.SYNC_TIME[56:0];
    end else if (sync) begin
      set <= 1'b0;
    end
  end

  always_ff @(posedge CLK) begin
    if (sync) begin
      skip_one_assert <= 1'b0;
      if (set & ~SYNC_SETTINGS.UPDATE) begin
        sys_time <= sync_time + 1;
        phase <= PhaseWidth'(1);
        sync_time_diff <= '0;
        locked <= 1'b1;
        resync_count <= '0;
      end else begin
        sys_time <= sys_time + 1;
        if (edge_overflow) begin
          phase <= PhaseWidth'(1);
          sync_time_diff <= '0;
          if (locked && (resync_count != 8'hFF)) resync_count <= resync_count + 8'd1;
        end else begin
          phase <= phase_next;
          sync_time_diff <= 14'(edge_diff);
        end
      end
    end else begin
      sys_time <= sys_time + 57'(step);
      phase <= phase_next;
      skip_one_assert <= step == 2'd2;
      if (step == 2'd0) sync_time_diff <= sync_time_diff + 1;
      else if (step == 2'd2) sync_time_diff <= sync_time_diff - 1;
    end
  end

  always_ff @(posedge CLK) begin
    if (adjust_cnt == '0) begin
      xor_t <= xor_x ^ {xor_x[20:0], 11'd0};
    end else if (adjust_cnt == adjust_cnt_cyc) begin
      xor_x <= xor_y;
      xor_y <= xor_z;
      xor_z <= xor_w;
      xor_w <= (xor_w ^ {19'd0, xor_w[31:19]}) ^ (xor_t ^ {8'd0, xor_t[31:8]});
      adjust_cnt_cyc <= {1'b0, xor_w[11:0]} + 13'(AdjustCntOffset);
    end
  end

  always_ff @(posedge CLK) adjust_cnt <= adjust_cnt == adjust_cnt_cyc ? '0 : adjust_cnt + 1;

  always_ff @(posedge CLK) sync_tri <= {sync_tri[1:0], SYNC_IN};

endmodule
`default_nettype wire
