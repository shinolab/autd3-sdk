`timescale 1ns / 1ps
module sim_time_cnt_generator ();

  `include "define.vh"

  localparam int T = 512;

  logic CLK = 1'b0;
  always #25 CLK = ~CLK;

  logic [56:0] sys_time = '0;
  logic skip_one_assert = 1'b0;
  logic [8:0] time_cnt;
  logic update;

  time_cnt_generator time_cnt_generator (
      .CLK(CLK),
      .SYS_TIME(sys_time),
      .SKIP_ONE_ASSERT(skip_one_assert),
      .TIME_CNT(time_cnt),
      .UPDATE(update)
  );

  int update_cnt = 0;
  always @(posedge CLK) begin
    if (update) begin
      update_cnt <= update_cnt + 1;
      #1 `ASSERT_EQ(1'b1, ((time_cnt == 9'd0) || (time_cnt == 9'd1)));
    end
  end

  task automatic step(int n);
    @(negedge CLK);
    sys_time = sys_time + 57'(n);
    skip_one_assert = n == 2;
  endtask

  task automatic inject_at(longint at, int n);
    while (sys_time < at) step(1);
    step(n);
  endtask

  initial begin
    inject_at(4 * T, 0);
    inject_at(6 * T - 1, 0);
    inject_at(8 * T + 1, 0);
    inject_at(10 * T - 1, 2);
    inject_at(12 * T - 2, 2);
    inject_at(14 * T, 2);
    inject_at(16 * T + 4, 1);
    repeat (4) @(posedge CLK);

    `ASSERT_EQ(sys_time[56:9], update_cnt);
    $display("OK!");
    $finish();
  end

endmodule
