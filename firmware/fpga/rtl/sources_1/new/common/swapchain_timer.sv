`timescale 1ns / 1ps
`default_nettype none
module swapchain_timer (
    input wire CLK,
    input wire UPDATE_SETTINGS_IN,
    input wire [56:0] SYS_TIME,
    input wire [15:0] CYCLE[params::NumBanks],
    input wire [15:0] FREQ_DIV[params::NumBanks],
    output wire [15:0] IDX[params::NumBanks],
    output wire UPDATE_SETTINGS_OUT
);

  logic update_settings = 1'b0;
  logic update_pending = 1'b0;

  typedef enum logic {
    IDLE,
    LOAD
  } state_t;

  state_t state = IDLE;

  logic   load_settings;
  assign load_settings = (state == IDLE) & (UPDATE_SETTINGS_IN | update_pending);


  logic marker = 1'b0;
  always_ff @(posedge CLK) if (load_settings) marker <= ~marker;

  logic idx_dout_valid[params::NumBanks];
  logic out_marker[params::NumBanks];

  assign UPDATE_SETTINGS_OUT = update_settings;

  for (genvar i = 0; i < params::NumBanks; i++) begin : gen_swapchain_timer_idx
    logic [15:0] freq_div;
    logic [16:0] cycle;
    logic [47:0] quo;
    logic [23:0] _unused_rem;
    logic [47:0] _unused_quo;
    logic [23:0] rem;
    logic cnt_marker;
    logic [15:0] idx;

    assign IDX[i] = idx;

    always_ff @(posedge CLK) begin
      idx <= (idx_dout_valid[i]) ? rem[15:0] : idx;
      if (load_settings) begin
        freq_div <= FREQ_DIV[i];
        cycle <= CYCLE[i] + 1;
      end
    end

    div_48_24 div_cnt (
        .s_axis_dividend_tdata(SYS_TIME[56:9]),
        .s_axis_dividend_tvalid(1'b1),
        .s_axis_dividend_tuser(marker),
        .s_axis_dividend_tready(),
        .s_axis_divisor_tdata({8'd0, freq_div}),
        .s_axis_divisor_tvalid(1'b1),
        .s_axis_divisor_tready(),
        .aclk(CLK),
        .m_axis_dout_tdata({quo, _unused_rem}),
        .m_axis_dout_tuser(cnt_marker),
        .m_axis_dout_tvalid()
    );
    div_48_24 div_idx (
        .s_axis_dividend_tdata(quo),
        .s_axis_dividend_tvalid(1'b1),
        .s_axis_dividend_tuser(cnt_marker),
        .s_axis_dividend_tready(),
        .s_axis_divisor_tdata({7'd0, cycle}),
        .s_axis_divisor_tvalid(1'b1),
        .s_axis_divisor_tready(),
        .aclk(CLK),
        .m_axis_dout_tdata({_unused_quo, rem}),
        .m_axis_dout_tuser(out_marker[i]),
        .m_axis_dout_tvalid(idx_dout_valid[i])
    );
  end

  logic marker_captured = 1'b0;

  always_ff @(posedge CLK) begin
    case (state)
      IDLE: begin
        update_settings <= 1'b0;
        if (UPDATE_SETTINGS_IN | update_pending) begin
          update_pending <= 1'b0;
          state <= LOAD;
        end
      end
      LOAD: begin
        update_pending <= update_pending | UPDATE_SETTINGS_IN;
        if (idx_dout_valid[0] && (out_marker[0] != marker_captured)) begin
          marker_captured <= out_marker[0];
          update_settings <= 1'b1;
          state <= IDLE;
        end
      end
      default: state <= IDLE;
    endcase
  end

endmodule
`default_nettype wire
