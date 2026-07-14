`timescale 1ns / 1ps
`default_nettype none
module synchronizer (
    input wire CLK,
    input wire settings::sync_settings_t SYNC_SETTINGS,
    input wire ECAT_SYNC,
    output var [56:0] SYS_TIME,
    output var SYNC,
    output var SKIP_ONE_ASSERT,
    output var signed [13:0] SYNC_TIME_DIFF
);

  localparam int AddSubLatency = 5;

  localparam int AdjustCntBase = 3125;
  localparam int AdjustCntRange = 4096;
  localparam int AdjustCntOffset = AdjustCntBase - AdjustCntRange / 2;

  localparam logic signed [57:0] DiffMax = 58'sd8191;

  logic [63:0] ecat_sync_time = '0;
  logic [31:0] ecat_sync_cycle = '0;
  logic [56:0] sync_time;

  logic [56:0] cycle_ticks;
  assign cycle_ticks = {25'd0, ecat_sync_cycle};

  (* ASYNC_REG = "true" *) logic [2:0] sync_tri = '0;
  logic sync;
  assign SYNC = sync;

  logic [56:0] sys_time = '0;
  logic [56:0] next_sync_time = '0;
  logic signed [13:0] sync_time_diff = '0;
  logic [$clog2(AddSubLatency+1)-1:0] diff_cnt = '0;
  logic [$clog2(AddSubLatency+1)-1:0] next_cnt = '0;
  logic set = 1'b0;

  // While `set` is armed but the ec_time -> sys_time conversion of the new
  // ECAT_SYNC_TIME has not settled yet, Sync0 pulses must not load the stale
  // conversion result. Each skipped pulse instead accumulates one Sync0
  // period into `pending_offset` so that a later load still snaps to the
  // correct pulse time.
  logic conv_settling = 1'b0;
  logic conv_settle_cnt = 1'b0;
  logic sync_time_dout_valid;
  logic [56:0] pending_offset = '0;
  logic [56:0] sync_time_adj = '0;

  logic [56:0] a_diff, b_diff;
  logic signed [57:0] s_diff;
  logic [56:0] a_next = 0, b_next, s_next;

  logic skip_one_assert;
  assign SKIP_ONE_ASSERT = skip_one_assert;
  assign SYNC_TIME_DIFF  = sync_time_diff;

  logic [31:0] xor_x = 32'd123456789;
  logic [31:0] xor_y = 32'd362436069;
  logic [31:0] xor_z = 32'd521288629;
  logic [31:0] xor_w = 32'd88675123;
  logic [31:0] xor_t;

  logic [12:0] adjust_cnt = '0;
  logic [12:0] adjust_cnt_cyc = 13'(AdjustCntBase);

  ec_time_to_sys_time ec_time_to_sys_time (
      .CLK(CLK),
      .EC_TIME(ecat_sync_time),
      .DIN_VALID(1'b1),
      .SYS_TIME(sync_time),
      .DOUT_VALID(sync_time_dout_valid)
  );

  sub57_57 sub_diff (
      .CLK(CLK),
      .A  (a_diff),
      .B  (b_diff),
      .S  (s_diff)
  );

  add57_57 add_next (
      .CLK(CLK),
      .A  (a_next),
      .B  (b_next),
      .S  (s_next)
  );

  assign sync = sync_tri[2:1] == 2'b01;
  assign SYS_TIME = sys_time;

  always_ff @(posedge CLK) begin
    if (SYNC_SETTINGS.UPDATE) begin
      set <= 1'b1;
      conv_settling <= 1'b1;
      conv_settle_cnt <= 1'b0;
      pending_offset <= '0;
      ecat_sync_time <= SYNC_SETTINGS.ECAT_SYNC_TIME;
      ecat_sync_cycle <= SYNC_SETTINGS.ECAT_SYNC_CYCLE;
    end else begin
      // The first DOUT_VALID after UPDATE may belong to a conversion that
      // sampled the old ECAT_SYNC_TIME; the second one is guaranteed fresh.
      if (conv_settling & sync_time_dout_valid) begin
        conv_settle_cnt <= 1'b1;
        if (conv_settle_cnt) conv_settling <= 1'b0;
      end
      if (sync & set) begin
        if (conv_settling) begin
          pending_offset <= pending_offset + cycle_ticks;
        end else begin
          set <= 1'b0;
        end
      end
    end
  end

  always_ff @(posedge CLK) sync_time_adj <= sync_time + pending_offset;

  always_ff @(posedge CLK) begin
    if (sync) begin
      b_next   <= cycle_ticks;
      next_cnt <= 0;
      if (set & ~conv_settling) begin
        sys_time <= sync_time_adj + 1;
        a_diff <= '0;
        b_diff <= '0;
        a_next <= sync_time_adj;
        sync_time_diff <= '0;
      end else begin
        a_diff   <= next_sync_time;
        b_diff   <= sys_time;
        a_next   <= next_sync_time;
        sys_time <= sys_time + 1;
      end
      diff_cnt <= '0;
      skip_one_assert <= 1'b0;
    end else begin
      if (diff_cnt == AddSubLatency + 1) begin
        if ((adjust_cnt != '0) || (sync_time_diff == '0)) begin
          sys_time <= sys_time + 1;
          skip_one_assert <= 1'b0;
        end else if (sync_time_diff < 14'sd0) begin
          sys_time <= sys_time;
          skip_one_assert <= 1'b0;
          sync_time_diff <= sync_time_diff + 1;
        end else begin
          sys_time <= sys_time + 2;
          skip_one_assert <= 1'b1;
          sync_time_diff <= sync_time_diff - 1;
        end
      end else if (diff_cnt == AddSubLatency) begin
        sync_time_diff <= saturate_diff(s_diff);
        diff_cnt <= diff_cnt + 1;
        sys_time <= sys_time + 1;
        skip_one_assert <= 1'b0;
      end else begin
        diff_cnt <= diff_cnt + 1;
        sys_time <= sys_time + 1;
        skip_one_assert <= 1'b0;
      end

      if (next_cnt == AddSubLatency + 1) begin
        next_cnt <= next_cnt;
      end else if (next_cnt == AddSubLatency) begin
        next_sync_time <= s_next;
        next_cnt <= next_cnt + 1;
      end else begin
        next_cnt <= next_cnt + 1;
      end
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

  always_ff @(posedge CLK) sync_tri <= {sync_tri[1:0], ECAT_SYNC};

  function automatic logic signed [13:0] saturate_diff(input logic signed [57:0] diff);
    if (diff > DiffMax) begin
      saturate_diff = 14'sd8191;
    end else if (diff < -DiffMax) begin
      saturate_diff = -14'sd8191;
    end else begin
      saturate_diff = 14'(diff);
    end
  endfunction

endmodule
`default_nettype wire
