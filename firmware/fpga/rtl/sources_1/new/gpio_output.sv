`timescale 1ns / 1ps
`default_nettype none
module gpio_output #(
    parameter int DEPTH = 249
) (
    input wire CLK,
    settings::debug_settings_t DEBUG_SETTINGS,
    input wire [8:0] TIME_CNT,
    input wire [56:0] SYS_TIME,
    input var signed [13:0] SYNC_TIME_DIFF,
    input wire PWM_OUT[DEPTH],
    input wire THERMO,
    input wire FORCE_FAN,
    input wire SYNC,
    input wire SYNC_IN_RAW,
    input wire PATTERN_BANK,
    input wire MOD_BANK,
    input wire [15:0] PATTERN_IDX,
    input wire [15:0] MOD_IDX,
    input wire [15:0] PATTERN_CYCLE,
    output wire GPIO_OUT[4]
);

  logic [63:0] debug_value[4] = '{4{{params::GPIO_O_TYPE_NONE, 56'd0}}};
  logic gpio_out[4];
  logic sync_raw_sel[4] = '{4{1'b0}};

  for (genvar i = 0; i < 4; i++) begin : gen_out
    assign GPIO_OUT[i] = sync_raw_sel[i] ? SYNC_IN_RAW : gpio_out[i];
  end

  always_ff @(posedge CLK) begin
    if (DEBUG_SETTINGS.UPDATE) debug_value <= DEBUG_SETTINGS.VALUE;
    for (int i = 0; i < 4; i++) begin
      sync_raw_sel[i] <= debug_value[i][63:56] == params::GPIO_O_TYPE_SYNC_RAW;
      gpio_out[i] <= debug_signal(debug_value[i][63:56], debug_value[i][55:0]);
    end
  end

  function automatic logic debug_signal(input logic [7:0] o_type, input logic [55:0] value);
    case (o_type)
      params::GPIO_O_TYPE_BASE_SIG: return TIME_CNT[8] == 1'b0;
      params::GPIO_O_TYPE_THERMO: return THERMO;
      params::GPIO_O_TYPE_FORCE_FAN: return FORCE_FAN;
      params::GPIO_O_TYPE_SYNC: return SYNC;
      params::GPIO_O_TYPE_MOD_BANK: return MOD_BANK;
      params::GPIO_O_TYPE_MOD_IDX: return MOD_IDX == value[15:0];
      params::GPIO_O_TYPE_PATTERN_BANK: return PATTERN_BANK;
      params::GPIO_O_TYPE_PATTERN_IDX: return PATTERN_IDX == value[15:0];
      params::GPIO_O_TYPE_IS_STM_MODE: return PATTERN_CYCLE != '0;
      params::GPIO_O_TYPE_SYS_TIME_EQ: return SYS_TIME[56:9] == value[47:0];
      params::GPIO_O_TYPE_SYNC_DIFF: return SYNC_TIME_DIFF != 14'sd0;
      params::GPIO_O_TYPE_PWM_OUT: return PWM_OUT[(value[7:0]<8'(DEPTH))?value[7:0] : 8'(DEPTH-1)];
      params::GPIO_O_TYPE_DIRECT: return value[0];
      default: return 1'b0;
    endcase
  endfunction

endmodule
`default_nettype wire
