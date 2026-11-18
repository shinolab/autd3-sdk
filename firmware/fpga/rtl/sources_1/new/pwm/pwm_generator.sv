`timescale 1ns / 1ps
`default_nettype none
module pwm_generator (
    input wire CLK,
    input wire [8:0] TIME_CNT,
    input wire [8:0] RISE,
    input wire [8:0] FALL,
    output var PWM_OUT
);

  always_ff @(posedge CLK) begin
    if (RISE <= FALL) begin
      PWM_OUT <= (RISE <= TIME_CNT) & (TIME_CNT < FALL);
    end else begin
      PWM_OUT <= (TIME_CNT < FALL) | (RISE <= TIME_CNT);
    end
  end

endmodule
`default_nettype wire
