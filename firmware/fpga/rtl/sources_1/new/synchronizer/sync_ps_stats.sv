`timescale 1ns / 1ps
`default_nettype none
module sync_ps_stats (
    input wire CLK,
    input wire settings::sync_ps_status_t STATUS,
    input wire SNAPSHOT,
    input wire CLEAR,
    output var signed [15:0] DIFF_MIN,
    output var signed [15:0] DIFF_MAX,
    output var [15:0] DIFF_OUTLIERS,
    output var signed [15:0] RATE_MIN,
    output var signed [15:0] RATE_MAX,
    output var [15:0] FINE_EXITS
);

  localparam logic signed [15:0] DiffMinEmpty = 16'sh7FFF;
  localparam logic signed [15:0] DiffMaxEmpty = 16'sh8000;

  logic signed [15:0] diff_min = DiffMinEmpty;
  logic signed [15:0] diff_max = DiffMaxEmpty;
  logic [15:0] diff_outliers = '0;
  logic signed [15:0] rate_min = '0;
  logic signed [15:0] rate_max = '0;
  logic [15:0] fine_exits = '0;
  logic fine_prev = 1'b0;

  logic signed [15:0] diff_min_hold = DiffMinEmpty;
  logic signed [15:0] diff_max_hold = DiffMaxEmpty;
  logic [15:0] diff_outliers_hold = '0;
  logic signed [15:0] rate_min_hold = '0;
  logic signed [15:0] rate_max_hold = '0;
  logic [15:0] fine_exits_hold = '0;

  assign DIFF_MIN = diff_min_hold;
  assign DIFF_MAX = diff_max_hold;
  assign DIFF_OUTLIERS = diff_outliers_hold;
  assign RATE_MIN = rate_min_hold;
  assign RATE_MAX = rate_max_hold;
  assign FINE_EXITS = fine_exits_hold;

  logic signed [15:0] diff;
  assign diff = 16'(STATUS.EDGE_DIFF);

  logic signed [15:0] diff_min_base;
  logic signed [15:0] diff_max_base;
  logic [15:0] diff_outliers_base;
  logic signed [15:0] rate_min_base;
  logic signed [15:0] rate_max_base;
  logic [15:0] fine_exits_base;
  assign diff_min_base = CLEAR ? DiffMinEmpty : diff_min;
  assign diff_max_base = CLEAR ? DiffMaxEmpty : diff_max;
  assign diff_outliers_base = CLEAR ? '0 : diff_outliers;
  assign rate_min_base = CLEAR ? STATUS.RATE : rate_min;
  assign rate_max_base = CLEAR ? STATUS.RATE : rate_max;
  assign fine_exits_base = CLEAR ? '0 : fine_exits;

  logic outlier;
  logic fine_exit;
  assign outlier   = STATUS.EDGE && ((diff > 16'sd1) || (diff < -16'sd1));
  assign fine_exit = fine_prev & ~STATUS.FINE;

  always_ff @(posedge CLK) begin
    fine_prev <= STATUS.FINE;
    diff_min <= (STATUS.EDGE && (diff < diff_min_base)) ? diff : diff_min_base;
    diff_max <= (STATUS.EDGE && (diff > diff_max_base)) ? diff : diff_max_base;
    diff_outliers <= diff_outliers_base + 16'(outlier && (diff_outliers_base != '1));
    rate_min <= (STATUS.RATE < rate_min_base) ? STATUS.RATE : rate_min_base;
    rate_max <= (STATUS.RATE > rate_max_base) ? STATUS.RATE : rate_max_base;
    fine_exits <= fine_exits_base + 16'(fine_exit && (fine_exits_base != '1));
  end

  always_ff @(posedge CLK) begin
    if (SNAPSHOT) begin
      diff_min_hold <= diff_min;
      diff_max_hold <= diff_max;
      diff_outliers_hold <= diff_outliers;
      rate_min_hold <= rate_min;
      rate_max_hold <= rate_max;
      fine_exits_hold <= fine_exits;
    end
  end

endmodule
`default_nettype wire
