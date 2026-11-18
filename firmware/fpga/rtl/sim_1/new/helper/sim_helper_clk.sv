`timescale 1ns / 1ps
module sim_helper_clk (
    output var CLK,
    output var LOCKED,
    output var [56:0] SYS_TIME
);

  initial begin
    CLK = '0;
    LOCKED = '0;
    SYS_TIME = 1;
    #500 LOCKED = '1;
  end

  always #24.414 CLK = ~CLK;

  always @(posedge CLK) SYS_TIME <= LOCKED ? SYS_TIME + 1 : SYS_TIME;

endmodule
