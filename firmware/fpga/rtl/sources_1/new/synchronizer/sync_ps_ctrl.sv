`timescale 1ns / 1ps
`default_nettype none
module sync_ps_ctrl (
    input wire CLK,
    input wire settings::sync_ps_settings_t SETTINGS,
    input wire EDGE,
    input wire FINE,
    input wire signed [13:0] EDGE_DIFF,
    output var signed [15:0] RATE,
    output var RATE_TOGGLE,
    output var settings::sync_ps_state_t STATE
);

  localparam int RateMax = 18000;
  localparam int FracBits = 8;
  localparam int IntegMax = RateMax << FracBits;
  localparam int FreqWindow = 16;
  localparam int ErrMax = 31;

  logic signed [31:0] integ = '0;
  logic signed [15:0] rate = '0;
  logic rate_toggle = 1'b0;

  logic enable_prev = 1'b0;
  logic fine_prev = 1'b0;
  logic [7:0] fine_exit_count = '0;
  logic [15:0] pd_pos = '0;
  logic [15:0] pd_neg = '0;
  logic signed [15:0] edge_diff_last = '0;
  logic [15:0] edge_count = '0;

  assign RATE = rate;
  assign RATE_TOGGLE = rate_toggle;

  assign STATE.FINE = FINE;
  assign STATE.FINE_EXIT_COUNT = fine_exit_count;
  assign STATE.RATE = rate;
  assign STATE.INTEG = 16'(integ >>> FracBits);
  assign STATE.PD_POS = pd_pos;
  assign STATE.PD_NEG = pd_neg;
  assign STATE.EDGE_DIFF = edge_diff_last;
  assign STATE.EDGE_COUNT = edge_count;

  logic signed [14:0] err_raw;
  logic signed [ 5:0] err;
  assign err_raw = 15'(EDGE_DIFF) * 15'sd2 + 15'sd1;
  assign err = (err_raw > ErrMax) ? 6'(ErrMax) : (err_raw < -ErrMax) ? 6'(-ErrMax) : 6'(err_raw);

  logic in_freq_window;
  assign in_freq_window = (EDGE_DIFF <= FreqWindow) && (EDGE_DIFF >= -FreqWindow);

  logic [15:0] gain;
  assign gain = FINE ? SETTINGS.KI : in_freq_window ? SETTINGS.KF : 16'd0;

  logic signed [22:0] integ_delta;
  logic signed [32:0] integ_sum;
  logic signed [31:0] integ_next;
  assign integ_delta = err * $signed({1'b0, gain});
  assign integ_sum   = 33'(integ) + 33'(integ_delta);
  assign integ_next  = (integ_sum > IntegMax) ? 32'(IntegMax) : (integ_sum < -IntegMax) ? 32'(-IntegMax) : 32'(integ_sum);

  logic signed [22:0] prop;
  logic signed [23:0] rate_sum;
  logic signed [15:0] rate_next;
  assign prop = FINE ? err * $signed({1'b0, SETTINGS.KP}) : 23'sd0;
  assign rate_sum = 24'(integ_next >>> FracBits) + 24'(prop);
  assign rate_next = (rate_sum > RateMax) ? 16'(RateMax) : (rate_sum < -RateMax) ? 16'(-RateMax) : 16'(rate_sum);

  logic signed [15:0] manual_rate;
  assign manual_rate = (SETTINGS.MANUAL_RATE > RateMax) ? 16'(RateMax) : (SETTINGS.MANUAL_RATE < -RateMax) ? 16'(-RateMax) : SETTINGS.MANUAL_RATE;

  always_ff @(posedge CLK) begin
    if (SETTINGS.UPDATE) begin
      if (~SETTINGS.ENABLE | SETTINGS.MANUAL) begin
        integ <= '0;
        rate <= (SETTINGS.ENABLE & SETTINGS.MANUAL) ? manual_rate : 16'sd0;
        rate_toggle <= ~rate_toggle;
      end
    end else if (EDGE & SETTINGS.ENABLE & ~SETTINGS.MANUAL) begin
      integ <= integ_next;
      rate <= rate_next;
      rate_toggle <= ~rate_toggle;
    end
  end

  always_ff @(posedge CLK) begin
    enable_prev <= SETTINGS.ENABLE;
    fine_prev   <= FINE;
    if (SETTINGS.ENABLE & ~enable_prev) begin
      fine_exit_count <= '0;
    end else if (fine_prev & ~FINE & SETTINGS.ENABLE & (fine_exit_count != 8'hFF)) begin
      fine_exit_count <= fine_exit_count + 8'd1;
    end
    if (EDGE) begin
      edge_count <= edge_count + 16'd1;
      edge_diff_last <= 16'(EDGE_DIFF);
      if (FINE) begin
        if (err > 0) pd_pos <= pd_pos + 16'd1;
        else pd_neg <= pd_neg + 16'd1;
      end
    end
  end

endmodule
`default_nettype wire
