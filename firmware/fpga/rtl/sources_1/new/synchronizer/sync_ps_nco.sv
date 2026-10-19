`timescale 1ns / 1ps
`default_nettype none
module sync_ps_nco (
    input wire PSCLK,
    input wire signed [15:0] RATE,
    input wire RATE_TOGGLE,
    input wire PSDONE,
    output var PSEN,
    output var PSINCDEC
);

  localparam int AccWidth = 18;
  localparam logic [5:0] BusyTimeout = 6'd63;

  logic rate_updated;

  cdc_toggle rate_cdc (
      .CLK(PSCLK),
      .TOGGLE(RATE_TOGGLE),
      .PULSE(rate_updated)
  );

  logic signed [15:0] rate = '0;
  logic [15:0] mag = '0;
  logic [AccWidth-1:0] acc = '0;
  logic carry = 1'b0;
  logic [2:0] pending = '0;
  logic busy = 1'b0;
  logic [5:0] busy_cnt = '0;
  logic psen = 1'b0;
  logic psincdec = 1'b0;

  assign PSEN = psen;
  assign PSINCDEC = psincdec;

  logic [AccWidth:0] acc_sum;
  logic issue;
  assign acc_sum = {1'b0, acc} + (AccWidth + 1)'(mag);
  assign issue   = ~busy & ~psen & (pending != 3'd0);

  always_ff @(posedge PSCLK) begin
    psen <= 1'b0;

    if (rate_updated) begin
      rate <= RATE;
    end
    mag   <= rate[15] ? 16'(-rate) : 16'(rate);
    acc   <= acc_sum[AccWidth-1:0];
    carry <= acc_sum[AccWidth];

    if (rate_updated & (RATE[15] != rate[15])) begin
      pending <= '0;
    end else begin
      pending <= pending + ((carry & (pending != 3'd7)) ? 3'd1 : 3'd0) - (issue ? 3'd1 : 3'd0);
    end

    if (issue) begin
      psen <= 1'b1;
      psincdec <= rate[15];
      busy <= 1'b1;
      busy_cnt <= '0;
    end else if (PSDONE | (busy_cnt == BusyTimeout)) begin
      busy <= 1'b0;
    end else if (busy) begin
      busy_cnt <= busy_cnt + 1'b1;
    end
  end

endmodule
`default_nettype wire
