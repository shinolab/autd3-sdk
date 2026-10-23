`timescale 1ns / 1ps
`default_nettype none
module sync_ps_ctrl (
    input wire CLK,
    input wire settings::sync_ps_settings_t SETTINGS,
    input wire EDGE,
    input wire FINE,
    input wire signed [13:0] EDGE_DIFF,
    output var signed [15:0] RATE,
    output var RATE_TOGGLE
);

  localparam int RateMax = 18000;
  localparam int FracBits = 8;
  localparam int IntegMax = RateMax << FracBits;
  localparam int FreqWindow = 16;
  localparam int ErrMax = 31;
  localparam logic [15:0] Kp = 16'd64;
  localparam logic [15:0] Ki = 16'd1024;
  localparam logic [15:0] Kf = 16'd4096;

  logic signed [31:0] integ = '0;
  logic signed [15:0] rate = '0;
  logic rate_toggle = 1'b0;

  assign RATE = rate;
  assign RATE_TOGGLE = rate_toggle;

  logic signed [14:0] err_raw;
  logic signed [ 5:0] err;
  assign err_raw = 15'(EDGE_DIFF) * 15'sd2 + 15'sd1;
  assign err = (err_raw > ErrMax) ? 6'(ErrMax) : (err_raw < -ErrMax) ? 6'(-ErrMax) : 6'(err_raw);

  logic in_freq_window;
  assign in_freq_window = (EDGE_DIFF <= FreqWindow) && (EDGE_DIFF >= -FreqWindow);

  logic [15:0] gain;
  assign gain = FINE ? Ki : in_freq_window ? Kf : 16'd0;

  logic signed [22:0] integ_delta;
  logic signed [32:0] integ_sum;
  logic signed [31:0] integ_next;
  assign integ_delta = err * $signed({1'b0, gain});
  assign integ_sum   = 33'(integ) + 33'(integ_delta);
  assign integ_next  = (integ_sum > IntegMax) ? 32'(IntegMax) : (integ_sum < -IntegMax) ? 32'(-IntegMax) : 32'(integ_sum);

  logic signed [22:0] prop;
  logic signed [23:0] rate_sum;
  logic signed [15:0] rate_next;
  assign prop = FINE ? err * $signed({1'b0, Kp}) : 23'sd0;
  assign rate_sum = 24'(integ_next >>> FracBits) + 24'(prop);
  assign rate_next = (rate_sum > RateMax) ? 16'(RateMax) : (rate_sum < -RateMax) ? 16'(-RateMax) : 16'(rate_sum);

  always_ff @(posedge CLK) begin
    if (SETTINGS.UPDATE) begin
      if (~SETTINGS.ENABLE) begin
        integ <= '0;
        rate <= 16'sd0;
        rate_toggle <= ~rate_toggle;
      end
    end else if (EDGE & SETTINGS.ENABLE) begin
      integ <= integ_next;
      rate <= rate_next;
      rate_toggle <= ~rate_toggle;
    end
  end

endmodule
`default_nettype wire
