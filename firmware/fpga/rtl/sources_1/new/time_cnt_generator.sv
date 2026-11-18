`timescale 1ns / 1ps
`default_nettype none
module time_cnt_generator (
    input wire CLK,
    input wire [56:0] SYS_TIME,
    input wire SKIP_ONE_ASSERT,
    output var [8:0] TIME_CNT,
    output var UPDATE
);
  logic [8:0] t_prev = '0;

  wire cross0 = (SYS_TIME[8:0] == 9'd0) & (t_prev != 9'd0);
  wire cross0_with_skip = (SYS_TIME[8:0] == 9'd1) & SKIP_ONE_ASSERT;

  always_ff @(posedge CLK) begin
    t_prev   <= SYS_TIME[8:0];
    TIME_CNT <= t_prev;
    UPDATE   <= cross0 | cross0_with_skip;
  end

endmodule
`default_nettype wire
