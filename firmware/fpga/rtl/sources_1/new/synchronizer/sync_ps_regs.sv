`timescale 1ns / 1ps
`default_nettype none
module sync_ps_regs (
    input wire BUS_CLK,
    input wire CLK,
    input wire EN,
    input wire WE,
    input wire [7:0] ADDR,
    input wire [15:0] DATA_IN,
    output var [15:0] DATA_OUT,
    output var settings::sync_ps_settings_t SETTINGS
);

  import params::*;

  logic ctl_reg = 1'b1;
  logic [3:0] write_count = '0;
  logic [3:0] write_count_gray = '0;
  logic [15:0] rdata = '0;

  logic [3:0] write_count_gray_sync;
  logic [3:0] write_count_gray_seen = '0;

  cdc_sync #(
      .WIDTH(4)
  ) write_count_cdc (
      .CLK(CLK),
      .D  (write_count_gray),
      .Q  (write_count_gray_sync)
  );

  logic update = 1'b0;
  logic enable = 1'b1;

  assign SETTINGS.UPDATE = update;
  assign SETTINGS.ENABLE = enable;

  always_ff @(posedge BUS_CLK) begin
    if (WE & EN) begin
      if (ADDR == ADDR_SYNC_PS_CTL) begin
        ctl_reg <= DATA_IN[0];
      end
      write_count <= write_count + 4'd1;
    end
    write_count_gray <= write_count ^ {1'b0, write_count[3:1]};
  end

  always_ff @(posedge CLK) begin
    update <= 1'b0;
    write_count_gray_seen <= write_count_gray_sync;
    if (write_count_gray_sync != write_count_gray_seen) begin
      enable <= ctl_reg;
      update <= 1'b1;
    end
  end

  assign DATA_OUT = rdata;

  always_ff @(posedge BUS_CLK) begin
    if (EN) begin
      rdata <= (ADDR == ADDR_SYNC_PS_CTL) ? {15'd0, ctl_reg} : 16'd0;
    end
  end

endmodule
`default_nettype wire
