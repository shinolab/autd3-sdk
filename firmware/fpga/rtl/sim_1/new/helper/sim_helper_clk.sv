`timescale 1ns / 1ps
module sim_helper_clk (
    output var CLK,
    output var LOCKED,
    output var [56:0] SYS_TIME
);

  logic clk;
  logic locked;
  logic [56:0] sys_time;

  assign CLK = clk;
  assign LOCKED = locked;
  assign SYS_TIME = sys_time;

  initial begin
    clk = '0;
    locked = '0;
    sys_time = 1;  // start with 1 to to prevent `time_cnt_generator::UPDATE` from being asserted
    #500 locked = '1;
  end

  always #24.414 clk = ~clk;

  always @(posedge clk) sys_time <= locked ? sys_time + 1 : sys_time;

endmodule
