`timescale 1ns / 1ps
`default_nettype none
module controller (
    input wire CLK,
    input wire ENABLE,
    input wire THERMO,
    input wire PATTERN_BANK,
    input wire MOD_BANK,
    input wire PATTERN_EXT_ACTIVE,
    input wire MOD_EXT_ACTIVE,
    input wire [15:0] PATTERN_CYCLE,
    input wire PATTERN_STOPPED,
    input wire MOD_STOPPED,
    input wire TRANSITION_PENDING,
    input wire [7:0] SYNC_RESYNC_COUNT,
    cnt_bus_if.out_port cnt_bus,
    output var settings::mod_settings_t MOD_SETTINGS,
    output var settings::pattern_settings_t PATTERN_SETTINGS,
    output var settings::silencer_settings_t SILENCER_SETTINGS,
    output var settings::sync_settings_t SYNC_SETTINGS,
    output var settings::debug_settings_t DEBUG_SETTINGS,
    output var FORCE_FAN,
    output var FAILSAFE,
    output var GPIO_IN[4]
);

  localparam bit [7:0] FunctionBits = (1'b0 << params::FuncDynamicFreqBit)
                                      | (1'b1 << params::FuncFlashOtaBit)
                                      | (1'b1 << params::FuncStrictSilencerGuardBit)
                                      | (1'b0 << params::FuncEmulatorBit);

  logic [15:0] ctl_flags = '0;
  logic [15:0] ctl_flags_cand = '0;
  logic [15:0] silencer_set_result = '0;

  logic we = 1'b0;
  logic [7:0] addr;
  logic [15:0] din;
  logic [15:0] dout;

  logic [15:0] fpga_state_prev = 16'hFFFF;

  assign cnt_bus.WE = we;
  assign cnt_bus.ADDR = addr;
  assign cnt_bus.DIN = din;
  assign dout = cnt_bus.DOUT;

  assign FORCE_FAN = ctl_flags[params::CTL_FLAG_BIT_FORCE_FAN];
  assign FAILSAFE = ctl_flags[params::CTL_FLAG_BIT_FAILSAFE];
  assign GPIO_IN[0] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_0];
  assign GPIO_IN[1] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_1];
  assign GPIO_IN[2] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_2];
  assign GPIO_IN[3] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_3];

  function automatic logic [15:0] fpga_state_din();
    logic [15:0] s = '0;
    s[params::FPGA_STATE_BIT_THERMAL_ASSERT] = THERMO;
    s[params::FPGA_STATE_BIT_MOD_BANK] = MOD_BANK;
    s[params::FPGA_STATE_BIT_PATTERN_BANK] = PATTERN_BANK;
    s[params::FPGA_STATE_BIT_PATTERN_MODE] = PATTERN_CYCLE == '0;
    s[params::FPGA_STATE_BIT_PATTERN_STOPPED] = PATTERN_STOPPED;
    s[params::FPGA_STATE_BIT_MOD_STOPPED] = MOD_STOPPED;
    s[params::FPGA_STATE_BIT_TRANSITION_PENDING] = TRANSITION_PENDING;
    s[params::FPGA_STATE_BIT_FAILSAFE] = ctl_flags[params::CTL_FLAG_BIT_FAILSAFE];
    s[15:8] = SYNC_RESYNC_COUNT;
    return s;
  endfunction

  settings::mod_settings_t mod_stage;
  settings::pattern_settings_t pattern_stage;
  settings::silencer_settings_t silencer_stage;

  function automatic logic is_strict(input logic [7:0] flag);
    return flag[params::SILENCER_FLAG_BIT_STRICT_MODE] & ~flag[params::SILENCER_FLAG_BIT_FIXED_UPDATE_RATE_MODE];
  endfunction

  logic mod_stage_finite;
  logic mod_now_finite;
  logic mod_stage_all;
  logic mod_now_all;
  logic [params::NumBanks-1:0] mod_stage_ng;
  logic [params::NumBanks-1:0] mod_now_ng;
  logic mod_reject;
  assign mod_stage_finite = mod_stage.REP[mod_stage.REQ_RD_BANK] != params::RepInfinite;
  assign mod_now_finite = MOD_SETTINGS.REP[MOD_SETTINGS.REQ_RD_BANK] != params::RepInfinite;
  assign mod_now_all = mod_now_finite ? MOD_EXT_ACTIVE : (MOD_SETTINGS.TRANSITION_MODE == params::TRANSITION_MODE_EXT);
  assign mod_stage_all = mod_stage_finite ? mod_now_all : (mod_stage.TRANSITION_MODE == params::TRANSITION_MODE_EXT);
  for (genvar b = 0; b < params::NumBanks; b++) begin : gen_mod_strict_guard
    logic stage_used;
    logic now_used;
    assign stage_used = mod_stage_all | (mod_stage.REQ_RD_BANK == 1'(b)) | (mod_stage_finite & (MOD_BANK == 1'(b)));
    assign now_used = mod_now_all | (MOD_SETTINGS.REQ_RD_BANK == 1'(b)) | (mod_now_finite & (MOD_BANK == 1'(b)));
    assign mod_stage_ng[b] = stage_used & (mod_stage.FREQ_DIV[b] < SILENCER_SETTINGS.COMPLETION_STEPS_INTENSITY);
    assign mod_now_ng[b] = now_used & (MOD_SETTINGS.FREQ_DIV[b] < silencer_stage.COMPLETION_STEPS_INTENSITY);
  end
  assign mod_reject = is_strict(SILENCER_SETTINGS.FLAG) & (|mod_stage_ng);

  logic pattern_stage_finite;
  logic pattern_now_finite;
  logic pattern_stage_all;
  logic pattern_now_all;
  logic [params::NumBanks-1:0] pattern_stage_ng;
  logic [params::NumBanks-1:0] pattern_now_ng;
  logic pattern_reject;
  assign pattern_stage_finite = pattern_stage.REP[pattern_stage.REQ_RD_BANK] != params::RepInfinite;
  assign pattern_now_finite = PATTERN_SETTINGS.REP[PATTERN_SETTINGS.REQ_RD_BANK] != params::RepInfinite;
  assign pattern_now_all = pattern_now_finite ? PATTERN_EXT_ACTIVE : (PATTERN_SETTINGS.TRANSITION_MODE == params::TRANSITION_MODE_EXT);
  assign pattern_stage_all = pattern_stage_finite ? pattern_now_all : (pattern_stage.TRANSITION_MODE == params::TRANSITION_MODE_EXT);
  for (genvar b = 0; b < params::NumBanks; b++) begin : gen_pattern_strict_guard
    logic stage_used;
    logic now_used;
    assign stage_used = pattern_stage_all | (pattern_stage.REQ_RD_BANK == 1'(b)) | (pattern_stage_finite & (PATTERN_BANK == 1'(b)));
    assign now_used = pattern_now_all | (PATTERN_SETTINGS.REQ_RD_BANK == 1'(b)) | (pattern_now_finite & (PATTERN_BANK == 1'(b)));
    assign pattern_stage_ng[b] = stage_used
        & ((pattern_stage.FREQ_DIV[b] < SILENCER_SETTINGS.COMPLETION_STEPS_INTENSITY)
           | (pattern_stage.FREQ_DIV[b] < SILENCER_SETTINGS.COMPLETION_STEPS_PHASE));
    assign pattern_now_ng[b] = now_used
        & ((PATTERN_SETTINGS.FREQ_DIV[b] < silencer_stage.COMPLETION_STEPS_INTENSITY)
           | (PATTERN_SETTINGS.FREQ_DIV[b] < silencer_stage.COMPLETION_STEPS_PHASE));
  end
  assign pattern_reject = is_strict(SILENCER_SETTINGS.FLAG) & (|pattern_stage_ng);

  logic silencer_reject;
  assign silencer_reject = is_strict(silencer_stage.FLAG) & ((|mod_now_ng) | (|pattern_now_ng));

  typedef enum logic [6:0] {
    REQ_WR_VER_PATCH,
    REQ_WR_VER_MINOR,
    REQ_WR_VER,
    REQ_WR_FUNCTION_BITS,
    WAIT_WR_VER_0_REQ_RD_CTL_FLAG,
    WR_VER_MINOR_WAIT_RD_CTL_FLAG_BIT_0,
    WR_VER_WAIT_RD_CTL_FLAG_BIT_1,
    WAIT_0,
    WAIT_1,
    SET_DONE_0,
    SET_DONE_1,
    REQ_MOD_REQ_RD_BANK,
    REQ_MOD_TRANSITION_MODE,
    REQ_MOD_TRANSITION_VALUE_0,
    REQ_MOD_TRANSITION_VALUE_1_RD_MOD_REQ_RD_BANK,
    REQ_MOD_TRANSITION_VALUE_2_RD_MOD_TRANSITION_MODE,
    REQ_MOD_TRANSITION_VALUE_3_RD_MOD_TRANSITION_VALUE_0,
    REQ_MOD_CYCLE0_RD_MOD_TRANSITION_VALUE_1,
    REQ_MOD_CYCLE1_RD_MOD_TRANSITION_VALUE_2,
    REQ_MOD_FREQ_DIV0_RD_MOD_TRANSITION_VALUE_3,
    REQ_MOD_FREQ_DIV1_RD_MOD_CYCLE0,
    REQ_MOD_REP0_RD_MOD_CYCLE1,
    REQ_MOD_REP1_RD_MOD_FREQ_DIV0,
    RD_MOD_FREQ_DIV1,
    RD_MOD_REP0,
    RD_MOD_REP1,
    MOD_VALIDATE,
    MOD_COMMIT,
    MOD_CLR_UPDATE_SETTINGS_BIT,
    REQ_PATTERN_REQ_RD_BANK,
    REQ_PATTERN_TRANSITION_MODE,
    REQ_PATTERN_TRANSITION_VALUE_0,
    REQ_PATTERN_TRANSITION_VALUE_1_RD_PATTERN_REQ_RD_BANK,
    REQ_PATTERN_TRANSITION_VALUE_2_RD_PATTERN_TRANSITION_MODE,
    REQ_PATTERN_TRANSITION_VALUE_3_RD_PATTERN_TRANSITION_VALUE_0,
    REQ_PATTERN_MODE0_RD_PATTERN_TRANSITION_VALUE_1,
    REQ_PATTERN_MODE1_RD_PATTERN_TRANSITION_VALUE_2,
    REQ_PATTERN_CYCLE0_RD_PATTERN_TRANSITION_VALUE_3,
    REQ_PATTERN_CYCLE1_RD_PATTERN_MODE0,
    REQ_PATTERN_FREQ_DIV0_RD_PATTERN_MODE1,
    REQ_PATTERN_FREQ_DIV1_RD_PATTERN_CYCLE0,
    REQ_PATTERN_SOUND_SPEED0_RD_PATTERN_CYCLE1,
    REQ_PATTERN_SOUND_SPEED1_RD_PATTERN_FREQ_DIV0,
    REQ_PATTERN_REP0_RD_PATTERN_FREQ_DIV1,
    REQ_PATTERN_REP1_RD_PATTERN_SOUND_SPEED0,
    REQ_PATTERN_NUM_FOCI0_RD_PATTERN_SOUND_SPEED1,
    REQ_PATTERN_NUM_FOCI1_RD_PATTERN_REP0,
    RD_PATTERN_REP1,
    RD_PATTERN_NUM_FOCI0,
    RD_PATTERN_NUM_FOCI1,
    PATTERN_VALIDATE,
    PATTERN_COMMIT,
    PATTERN_CLR_UPDATE_SETTINGS_BIT,
    REQ_SILENCER_FLAG,
    REQ_SILENCER_UPDATE_RATE_INTENSITY,
    REQ_SILENCER_UPDATE_RATE_PHASE,
    REQ_SILENCER_COMPLETION_STEPS_INTENSITY_RD_SILENCER_FLAG,
    REQ_SILENCER_COMPLETION_STEPS_PHASE_RD_SILENCER_UPDATE_RATE_INTENSITY,
    RD_SILENCER_UPDATE_RATE_PHASE,
    RD_SILENCER_COMPLETION_STEPS_INTENSITY,
    RD_SILENCER_COMPLETION_STEPS_PHASE,
    SILENCER_VALIDATE,
    SILENCER_COMMIT,
    SILENCER_CLR_UPDATE_SETTINGS_BIT,
    REQ_DEBUG_VALUE0_0,
    REQ_DEBUG_VALUE0_1,
    REQ_DEBUG_VALUE0_2,
    REQ_DEBUG_VALUE0_3_RD_DEBUG_VALUE0_0,
    REQ_DEBUG_VALUE1_0_RD_DEBUG_VALUE0_1,
    REQ_DEBUG_VALUE1_1_RD_DEBUG_VALUE0_2,
    REQ_DEBUG_VALUE1_2_RD_DEBUG_VALUE0_3,
    REQ_DEBUG_VALUE1_3_RD_DEBUG_VALUE1_0,
    REQ_DEBUG_VALUE2_0_RD_DEBUG_VALUE1_1,
    REQ_DEBUG_VALUE2_1_RD_DEBUG_VALUE1_2,
    REQ_DEBUG_VALUE2_2_RD_DEBUG_VALUE1_3,
    REQ_DEBUG_VALUE2_3_RD_DEBUG_VALUE2_0,
    REQ_DEBUG_VALUE3_0_RD_DEBUG_VALUE2_1,
    REQ_DEBUG_VALUE3_1_RD_DEBUG_VALUE2_2,
    REQ_DEBUG_VALUE3_2_RD_DEBUG_VALUE2_3,
    REQ_DEBUG_VALUE3_3_RD_DEBUG_VALUE3_0,
    RD_DEBUG_VALUE3_1,
    RD_DEBUG_VALUE3_2,
    RD_DEBUG_VALUE3_3,
    DEBUG_COMMIT,
    DEBUG_CLR_UPDATE_SETTINGS_BIT,
    REQ_SYNC_TIME_0,
    REQ_SYNC_TIME_1,
    REQ_SYNC_TIME_2,
    REQ_SYNC_TIME_3_RD_SYNC_TIME_0,
    RD_SYNC_TIME_1,
    RD_SYNC_TIME_2,
    RD_SYNC_TIME_3,
    SYNC_COMMIT,
    SYNC_CLR_UPDATE_SETTINGS_BIT
  } state_t;

  state_t state = REQ_WR_VER_PATCH;

  always_ff @(posedge CLK) begin
    if (!ENABLE) begin
      state <= REQ_WR_VER_PATCH;
      we <= 1'b0;
      fpga_state_prev <= 16'hFFFF;
      ctl_flags <= '0;
      ctl_flags_cand <= '0;
      MOD_SETTINGS.UPDATE <= 1'b0;
      PATTERN_SETTINGS.UPDATE <= 1'b0;
      SILENCER_SETTINGS.UPDATE <= 1'b0;
      DEBUG_SETTINGS.UPDATE <= 1'b0;
      SYNC_SETTINGS.UPDATE <= 1'b0;
    end else
      case (state)
        REQ_WR_VER_PATCH: begin
          we <= 1'b1;

          din <= {8'd0, params::VersionNumPatch};
          addr <= params::ADDR_VERSION_NUM_PATCH;

          state <= REQ_WR_VER_MINOR;
        end
        REQ_WR_VER_MINOR: begin
          din   <= {8'd0, params::VersionNumMinor};
          addr  <= params::ADDR_VERSION_NUM_MINOR;

          state <= REQ_WR_VER;
        end
        REQ_WR_VER: begin
          din   <= {8'd0, params::VersionNumMajor};
          addr  <= params::ADDR_VERSION_NUM_MAJOR;

          state <= REQ_WR_FUNCTION_BITS;
        end
        REQ_WR_FUNCTION_BITS: begin
          din   <= {8'd0, FunctionBits};
          addr  <= params::ADDR_FUNCTION_BITS;

          state <= WAIT_WR_VER_0_REQ_RD_CTL_FLAG;
        end
        WAIT_WR_VER_0_REQ_RD_CTL_FLAG: begin
          we <= 1'b0;
          addr <= params::ADDR_CTL_FLAG;

          state <= WR_VER_MINOR_WAIT_RD_CTL_FLAG_BIT_0;
        end
        WR_VER_MINOR_WAIT_RD_CTL_FLAG_BIT_0: begin
          state <= WR_VER_WAIT_RD_CTL_FLAG_BIT_1;
        end
        WR_VER_WAIT_RD_CTL_FLAG_BIT_1: begin
          state <= WAIT_0;
        end

        WAIT_0: begin
          addr <= params::ADDR_FPGA_STATE;
          if (fpga_state_din() != fpga_state_prev) begin
            we <= 1'b1;
            din <= fpga_state_din();
            fpga_state_prev <= fpga_state_din();
          end else begin
            we <= 1'b0;
          end

          if (ctl_flags[params::CTL_FLAG_BIT_MOD_SET]) begin
            ctl_flags <= ctl_flags & ~(1 << params::CTL_FLAG_BIT_MOD_SET);
            state <= REQ_MOD_REQ_RD_BANK;
          end else if (ctl_flags[params::CTL_FLAG_BIT_PATTERN_SET]) begin
            ctl_flags <= ctl_flags & ~(1 << params::CTL_FLAG_BIT_PATTERN_SET);
            state <= REQ_PATTERN_REQ_RD_BANK;
          end else if (ctl_flags[params::CTL_FLAG_BIT_SILENCER_SET]) begin
            ctl_flags <= ctl_flags & ~(1 << params::CTL_FLAG_BIT_SILENCER_SET);
            state <= REQ_SILENCER_FLAG;
          end else if (ctl_flags[params::CTL_FLAG_BIT_DEBUG_SET]) begin
            ctl_flags <= ctl_flags & ~(1 << params::CTL_FLAG_BIT_DEBUG_SET);
            state <= REQ_DEBUG_VALUE0_0;
          end else if (ctl_flags[params::CTL_FLAG_BIT_SYNC_SET]) begin
            ctl_flags <= ctl_flags & ~(1 << params::CTL_FLAG_BIT_SYNC_SET);
            state <= REQ_SYNC_TIME_0;
          end else begin
            ctl_flags_cand <= dout;
            if (dout == ctl_flags_cand) begin
              ctl_flags <= dout;
            end
            state <= WAIT_1;
          end
        end
        WAIT_1: begin
          we <= 1'b0;
          addr <= params::ADDR_CTL_FLAG;
          state <= WAIT_0;
        end
        SET_DONE_0: begin
          we <= 1'b0;
          state <= SET_DONE_1;
        end
        SET_DONE_1: begin
          addr  <= params::ADDR_FPGA_STATE;
          state <= WAIT_1;
        end

        REQ_MOD_REQ_RD_BANK: begin
          we <= 1'b0;
          addr <= params::ADDR_MOD_REQ_RD_BANK;
          state <= REQ_MOD_TRANSITION_MODE;
        end
        REQ_MOD_TRANSITION_MODE: begin
          addr  <= params::ADDR_MOD_TRANSITION_MODE;
          state <= REQ_MOD_TRANSITION_VALUE_0;
        end
        REQ_MOD_TRANSITION_VALUE_0: begin
          addr  <= params::ADDR_MOD_TRANSITION_VALUE_0;
          state <= REQ_MOD_TRANSITION_VALUE_1_RD_MOD_REQ_RD_BANK;
        end
        REQ_MOD_TRANSITION_VALUE_1_RD_MOD_REQ_RD_BANK: begin
          addr <= params::ADDR_MOD_TRANSITION_VALUE_1;
          mod_stage.REQ_RD_BANK <= dout[0];
          state <= REQ_MOD_TRANSITION_VALUE_2_RD_MOD_TRANSITION_MODE;
        end
        REQ_MOD_TRANSITION_VALUE_2_RD_MOD_TRANSITION_MODE: begin
          addr <= params::ADDR_MOD_TRANSITION_VALUE_2;
          mod_stage.TRANSITION_MODE <= dout[7:0];
          state <= REQ_MOD_TRANSITION_VALUE_3_RD_MOD_TRANSITION_VALUE_0;
        end
        REQ_MOD_TRANSITION_VALUE_3_RD_MOD_TRANSITION_VALUE_0: begin
          addr <= params::ADDR_MOD_TRANSITION_VALUE_3;
          mod_stage.TRANSITION_VALUE[15:0] <= dout;
          state <= REQ_MOD_CYCLE0_RD_MOD_TRANSITION_VALUE_1;
        end
        REQ_MOD_CYCLE0_RD_MOD_TRANSITION_VALUE_1: begin
          addr <= params::ADDR_MOD_CYCLE0;
          mod_stage.TRANSITION_VALUE[31:16] <= dout;
          state <= REQ_MOD_CYCLE1_RD_MOD_TRANSITION_VALUE_2;
        end
        REQ_MOD_CYCLE1_RD_MOD_TRANSITION_VALUE_2: begin
          addr <= params::ADDR_MOD_CYCLE1;
          mod_stage.TRANSITION_VALUE[47:32] <= dout;
          state <= REQ_MOD_FREQ_DIV0_RD_MOD_TRANSITION_VALUE_3;
        end
        REQ_MOD_FREQ_DIV0_RD_MOD_TRANSITION_VALUE_3: begin
          addr <= params::ADDR_MOD_FREQ_DIV0;
          mod_stage.TRANSITION_VALUE[63:48] <= dout;
          state <= REQ_MOD_FREQ_DIV1_RD_MOD_CYCLE0;
        end
        REQ_MOD_FREQ_DIV1_RD_MOD_CYCLE0: begin
          addr <= params::ADDR_MOD_FREQ_DIV1;
          mod_stage.CYCLE[0] <= dout;
          state <= REQ_MOD_REP0_RD_MOD_CYCLE1;
        end
        REQ_MOD_REP0_RD_MOD_CYCLE1: begin
          addr <= params::ADDR_MOD_REP0;
          mod_stage.CYCLE[1] <= dout;
          state <= REQ_MOD_REP1_RD_MOD_FREQ_DIV0;
        end
        REQ_MOD_REP1_RD_MOD_FREQ_DIV0: begin
          addr <= params::ADDR_MOD_REP1;
          mod_stage.FREQ_DIV[0] <= dout;
          state <= RD_MOD_FREQ_DIV1;
        end
        RD_MOD_FREQ_DIV1: begin
          mod_stage.FREQ_DIV[1] <= dout;
          state <= RD_MOD_REP0;
        end
        RD_MOD_REP0: begin
          mod_stage.REP[0] <= dout;
          state <= RD_MOD_REP1;
        end
        RD_MOD_REP1: begin
          mod_stage.REP[1] <= dout;
          state <= MOD_VALIDATE;
        end
        MOD_VALIDATE: begin
          silencer_set_result[params::CTL_FLAG_BIT_MOD_SET] <= mod_reject;
          state <= MOD_COMMIT;
        end
        MOD_COMMIT: begin
          if (!silencer_set_result[params::CTL_FLAG_BIT_MOD_SET]) begin
            MOD_SETTINGS.REQ_RD_BANK <= mod_stage.REQ_RD_BANK;
            MOD_SETTINGS.TRANSITION_MODE <= mod_stage.TRANSITION_MODE;
            MOD_SETTINGS.TRANSITION_VALUE <= mod_stage.TRANSITION_VALUE;
            MOD_SETTINGS.CYCLE[0] <= mod_stage.CYCLE[0];
            MOD_SETTINGS.CYCLE[1] <= mod_stage.CYCLE[1];
            MOD_SETTINGS.FREQ_DIV[0] <= mod_stage.FREQ_DIV[0];
            MOD_SETTINGS.FREQ_DIV[1] <= mod_stage.FREQ_DIV[1];
            MOD_SETTINGS.REP[0] <= mod_stage.REP[0];
            MOD_SETTINGS.REP[1] <= mod_stage.REP[1];
            MOD_SETTINGS.UPDATE <= 1'b1;
          end
          we <= 1'b1;
          addr <= params::ADDR_SILENCER_SET_RESULT;
          din <= silencer_set_result;
          state <= MOD_CLR_UPDATE_SETTINGS_BIT;
        end
        MOD_CLR_UPDATE_SETTINGS_BIT: begin
          MOD_SETTINGS.UPDATE <= 1'b0;
          we <= 1'b1;
          addr <= params::ADDR_CTL_FLAG;
          din <= ctl_flags;
          ctl_flags_cand <= ctl_flags;
          state <= SET_DONE_0;
        end

        REQ_PATTERN_REQ_RD_BANK: begin
          we <= 1'b0;
          addr <= params::ADDR_PATTERN_REQ_RD_BANK;
          state <= REQ_PATTERN_TRANSITION_MODE;
        end
        REQ_PATTERN_TRANSITION_MODE: begin
          addr  <= params::ADDR_PATTERN_TRANSITION_MODE;
          state <= REQ_PATTERN_TRANSITION_VALUE_0;
        end
        REQ_PATTERN_TRANSITION_VALUE_0: begin
          addr  <= params::ADDR_PATTERN_TRANSITION_VALUE_0;
          state <= REQ_PATTERN_TRANSITION_VALUE_1_RD_PATTERN_REQ_RD_BANK;
        end
        REQ_PATTERN_TRANSITION_VALUE_1_RD_PATTERN_REQ_RD_BANK: begin
          addr <= params::ADDR_PATTERN_TRANSITION_VALUE_1;
          pattern_stage.REQ_RD_BANK <= dout[0];
          state <= REQ_PATTERN_TRANSITION_VALUE_2_RD_PATTERN_TRANSITION_MODE;
        end
        REQ_PATTERN_TRANSITION_VALUE_2_RD_PATTERN_TRANSITION_MODE: begin
          addr <= params::ADDR_PATTERN_TRANSITION_VALUE_2;
          pattern_stage.TRANSITION_MODE <= dout[7:0];
          state <= REQ_PATTERN_TRANSITION_VALUE_3_RD_PATTERN_TRANSITION_VALUE_0;
        end
        REQ_PATTERN_TRANSITION_VALUE_3_RD_PATTERN_TRANSITION_VALUE_0: begin
          addr <= params::ADDR_PATTERN_TRANSITION_VALUE_3;
          pattern_stage.TRANSITION_VALUE[15:0] <= dout;
          state <= REQ_PATTERN_MODE0_RD_PATTERN_TRANSITION_VALUE_1;
        end
        REQ_PATTERN_MODE0_RD_PATTERN_TRANSITION_VALUE_1: begin
          addr <= params::ADDR_PATTERN_MODE0;
          pattern_stage.TRANSITION_VALUE[31:16] <= dout;
          state <= REQ_PATTERN_MODE1_RD_PATTERN_TRANSITION_VALUE_2;
        end
        REQ_PATTERN_MODE1_RD_PATTERN_TRANSITION_VALUE_2: begin
          addr <= params::ADDR_PATTERN_MODE1;
          pattern_stage.TRANSITION_VALUE[47:32] <= dout;
          state <= REQ_PATTERN_CYCLE0_RD_PATTERN_TRANSITION_VALUE_3;
        end
        REQ_PATTERN_CYCLE0_RD_PATTERN_TRANSITION_VALUE_3: begin
          addr <= params::ADDR_PATTERN_CYCLE0;
          pattern_stage.TRANSITION_VALUE[63:48] <= dout;
          state <= REQ_PATTERN_CYCLE1_RD_PATTERN_MODE0;
        end
        REQ_PATTERN_CYCLE1_RD_PATTERN_MODE0: begin
          addr <= params::ADDR_PATTERN_CYCLE1;
          pattern_stage.MODE[0] <= dout[0];
          state <= REQ_PATTERN_FREQ_DIV0_RD_PATTERN_MODE1;
        end
        REQ_PATTERN_FREQ_DIV0_RD_PATTERN_MODE1: begin
          addr <= params::ADDR_PATTERN_FREQ_DIV0;
          pattern_stage.MODE[1] <= dout[0];
          state <= REQ_PATTERN_FREQ_DIV1_RD_PATTERN_CYCLE0;
        end
        REQ_PATTERN_FREQ_DIV1_RD_PATTERN_CYCLE0: begin
          addr <= params::ADDR_PATTERN_FREQ_DIV1;
          pattern_stage.CYCLE[0] <= dout;
          state <= REQ_PATTERN_SOUND_SPEED0_RD_PATTERN_CYCLE1;
        end
        REQ_PATTERN_SOUND_SPEED0_RD_PATTERN_CYCLE1: begin
          addr <= params::ADDR_PATTERN_SOUND_SPEED0;
          pattern_stage.CYCLE[1] <= dout;
          state <= REQ_PATTERN_SOUND_SPEED1_RD_PATTERN_FREQ_DIV0;
        end
        REQ_PATTERN_SOUND_SPEED1_RD_PATTERN_FREQ_DIV0: begin
          addr <= params::ADDR_PATTERN_SOUND_SPEED1;
          pattern_stage.FREQ_DIV[0] <= dout;
          state <= REQ_PATTERN_REP0_RD_PATTERN_FREQ_DIV1;
        end
        REQ_PATTERN_REP0_RD_PATTERN_FREQ_DIV1: begin
          addr <= params::ADDR_PATTERN_REP0;
          pattern_stage.FREQ_DIV[1] <= dout;
          state <= REQ_PATTERN_REP1_RD_PATTERN_SOUND_SPEED0;
        end
        REQ_PATTERN_REP1_RD_PATTERN_SOUND_SPEED0: begin
          addr <= params::ADDR_PATTERN_REP1;
          pattern_stage.SOUND_SPEED[0] <= dout;
          state <= REQ_PATTERN_NUM_FOCI0_RD_PATTERN_SOUND_SPEED1;
        end
        REQ_PATTERN_NUM_FOCI0_RD_PATTERN_SOUND_SPEED1: begin
          addr <= params::ADDR_PATTERN_NUM_FOCI0;
          pattern_stage.SOUND_SPEED[1] <= dout;
          state <= REQ_PATTERN_NUM_FOCI1_RD_PATTERN_REP0;
        end
        REQ_PATTERN_NUM_FOCI1_RD_PATTERN_REP0: begin
          addr <= params::ADDR_PATTERN_NUM_FOCI1;
          pattern_stage.REP[0] <= dout;
          state <= RD_PATTERN_REP1;
        end
        RD_PATTERN_REP1: begin
          pattern_stage.REP[1] <= dout;
          state <= RD_PATTERN_NUM_FOCI0;
        end
        RD_PATTERN_NUM_FOCI0: begin
          pattern_stage.NUM_FOCI[0] <= dout[7:0];
          state <= RD_PATTERN_NUM_FOCI1;
        end
        RD_PATTERN_NUM_FOCI1: begin
          pattern_stage.NUM_FOCI[1] <= dout[7:0];
          state <= PATTERN_VALIDATE;
        end
        PATTERN_VALIDATE: begin
          silencer_set_result[params::CTL_FLAG_BIT_PATTERN_SET] <= pattern_reject;
          state <= PATTERN_COMMIT;
        end
        PATTERN_COMMIT: begin
          if (!silencer_set_result[params::CTL_FLAG_BIT_PATTERN_SET]) begin
            PATTERN_SETTINGS.REQ_RD_BANK <= pattern_stage.REQ_RD_BANK;
            PATTERN_SETTINGS.TRANSITION_MODE <= pattern_stage.TRANSITION_MODE;
            PATTERN_SETTINGS.TRANSITION_VALUE <= pattern_stage.TRANSITION_VALUE;
            PATTERN_SETTINGS.MODE[0] <= pattern_stage.MODE[0];
            PATTERN_SETTINGS.MODE[1] <= pattern_stage.MODE[1];
            PATTERN_SETTINGS.CYCLE[0] <= pattern_stage.CYCLE[0];
            PATTERN_SETTINGS.CYCLE[1] <= pattern_stage.CYCLE[1];
            PATTERN_SETTINGS.FREQ_DIV[0] <= pattern_stage.FREQ_DIV[0];
            PATTERN_SETTINGS.FREQ_DIV[1] <= pattern_stage.FREQ_DIV[1];
            PATTERN_SETTINGS.SOUND_SPEED[0] <= pattern_stage.SOUND_SPEED[0];
            PATTERN_SETTINGS.SOUND_SPEED[1] <= pattern_stage.SOUND_SPEED[1];
            PATTERN_SETTINGS.REP[0] <= pattern_stage.REP[0];
            PATTERN_SETTINGS.REP[1] <= pattern_stage.REP[1];
            PATTERN_SETTINGS.NUM_FOCI[0] <= pattern_stage.NUM_FOCI[0];
            PATTERN_SETTINGS.NUM_FOCI[1] <= pattern_stage.NUM_FOCI[1];
            PATTERN_SETTINGS.UPDATE <= 1'b1;
          end
          we <= 1'b1;
          addr <= params::ADDR_SILENCER_SET_RESULT;
          din <= silencer_set_result;
          state <= PATTERN_CLR_UPDATE_SETTINGS_BIT;
        end
        PATTERN_CLR_UPDATE_SETTINGS_BIT: begin
          PATTERN_SETTINGS.UPDATE <= 1'b0;
          we <= 1'b1;
          addr <= params::ADDR_CTL_FLAG;
          din <= ctl_flags;
          ctl_flags_cand <= ctl_flags;
          state <= SET_DONE_0;
        end

        REQ_SILENCER_FLAG: begin
          we <= 1'b0;
          addr <= params::ADDR_SILENCER_FLAG;
          state <= REQ_SILENCER_UPDATE_RATE_INTENSITY;
        end
        REQ_SILENCER_UPDATE_RATE_INTENSITY: begin
          addr  <= params::ADDR_SILENCER_UPDATE_RATE_INTENSITY;
          state <= REQ_SILENCER_UPDATE_RATE_PHASE;
        end
        REQ_SILENCER_UPDATE_RATE_PHASE: begin
          addr  <= params::ADDR_SILENCER_UPDATE_RATE_PHASE;
          state <= REQ_SILENCER_COMPLETION_STEPS_INTENSITY_RD_SILENCER_FLAG;
        end
        REQ_SILENCER_COMPLETION_STEPS_INTENSITY_RD_SILENCER_FLAG: begin
          addr <= params::ADDR_SILENCER_COMPLETION_STEPS_INTENSITY;
          silencer_stage.FLAG <= dout[7:0];
          state <= REQ_SILENCER_COMPLETION_STEPS_PHASE_RD_SILENCER_UPDATE_RATE_INTENSITY;
        end
        REQ_SILENCER_COMPLETION_STEPS_PHASE_RD_SILENCER_UPDATE_RATE_INTENSITY: begin
          addr <= params::ADDR_SILENCER_COMPLETION_STEPS_PHASE;
          silencer_stage.UPDATE_RATE_INTENSITY <= dout;
          state <= RD_SILENCER_UPDATE_RATE_PHASE;
        end
        RD_SILENCER_UPDATE_RATE_PHASE: begin
          silencer_stage.UPDATE_RATE_PHASE <= dout;
          state <= RD_SILENCER_COMPLETION_STEPS_INTENSITY;
        end
        RD_SILENCER_COMPLETION_STEPS_INTENSITY: begin
          silencer_stage.COMPLETION_STEPS_INTENSITY <= dout;
          state <= RD_SILENCER_COMPLETION_STEPS_PHASE;
        end
        RD_SILENCER_COMPLETION_STEPS_PHASE: begin
          silencer_stage.COMPLETION_STEPS_PHASE <= dout;
          state <= SILENCER_VALIDATE;
        end
        SILENCER_VALIDATE: begin
          silencer_set_result[params::CTL_FLAG_BIT_SILENCER_SET] <= silencer_reject;
          state <= SILENCER_COMMIT;
        end
        SILENCER_COMMIT: begin
          if (!silencer_set_result[params::CTL_FLAG_BIT_SILENCER_SET]) begin
            SILENCER_SETTINGS.FLAG <= silencer_stage.FLAG;
            SILENCER_SETTINGS.UPDATE_RATE_INTENSITY <= silencer_stage.UPDATE_RATE_INTENSITY;
            SILENCER_SETTINGS.UPDATE_RATE_PHASE <= silencer_stage.UPDATE_RATE_PHASE;
            SILENCER_SETTINGS.COMPLETION_STEPS_INTENSITY <= silencer_stage.COMPLETION_STEPS_INTENSITY;
            SILENCER_SETTINGS.COMPLETION_STEPS_PHASE <= silencer_stage.COMPLETION_STEPS_PHASE;
            SILENCER_SETTINGS.UPDATE <= 1'b1;
          end
          we <= 1'b1;
          addr <= params::ADDR_SILENCER_SET_RESULT;
          din <= silencer_set_result;
          state <= SILENCER_CLR_UPDATE_SETTINGS_BIT;
        end
        SILENCER_CLR_UPDATE_SETTINGS_BIT: begin
          SILENCER_SETTINGS.UPDATE <= 1'b0;
          we <= 1'b1;
          addr <= params::ADDR_CTL_FLAG;
          din <= ctl_flags;
          ctl_flags_cand <= ctl_flags;
          state <= SET_DONE_0;
        end

        REQ_DEBUG_VALUE0_0: begin
          we <= 1'b0;
          addr <= params::ADDR_DEBUG_VALUE0_0;
          state <= REQ_DEBUG_VALUE0_1;
        end
        REQ_DEBUG_VALUE0_1: begin
          addr  <= params::ADDR_DEBUG_VALUE0_1;
          state <= REQ_DEBUG_VALUE0_2;
        end
        REQ_DEBUG_VALUE0_2: begin
          addr  <= params::ADDR_DEBUG_VALUE0_2;
          state <= REQ_DEBUG_VALUE0_3_RD_DEBUG_VALUE0_0;
        end
        REQ_DEBUG_VALUE0_3_RD_DEBUG_VALUE0_0: begin
          addr <= params::ADDR_DEBUG_VALUE0_3;
          DEBUG_SETTINGS.VALUE[0][15:0] <= dout;
          state <= REQ_DEBUG_VALUE1_0_RD_DEBUG_VALUE0_1;
        end
        REQ_DEBUG_VALUE1_0_RD_DEBUG_VALUE0_1: begin
          addr <= params::ADDR_DEBUG_VALUE1_0;
          DEBUG_SETTINGS.VALUE[0][31:16] <= dout;
          state <= REQ_DEBUG_VALUE1_1_RD_DEBUG_VALUE0_2;
        end
        REQ_DEBUG_VALUE1_1_RD_DEBUG_VALUE0_2: begin
          addr <= params::ADDR_DEBUG_VALUE1_1;
          DEBUG_SETTINGS.VALUE[0][47:32] <= dout;
          state <= REQ_DEBUG_VALUE1_2_RD_DEBUG_VALUE0_3;
        end
        REQ_DEBUG_VALUE1_2_RD_DEBUG_VALUE0_3: begin
          addr <= params::ADDR_DEBUG_VALUE1_2;
          DEBUG_SETTINGS.VALUE[0][63:48] <= dout;
          state <= REQ_DEBUG_VALUE1_3_RD_DEBUG_VALUE1_0;
        end
        REQ_DEBUG_VALUE1_3_RD_DEBUG_VALUE1_0: begin
          addr <= params::ADDR_DEBUG_VALUE1_3;
          DEBUG_SETTINGS.VALUE[1][15:0] <= dout;
          state <= REQ_DEBUG_VALUE2_0_RD_DEBUG_VALUE1_1;
        end
        REQ_DEBUG_VALUE2_0_RD_DEBUG_VALUE1_1: begin
          addr <= params::ADDR_DEBUG_VALUE2_0;
          DEBUG_SETTINGS.VALUE[1][31:16] <= dout;
          state <= REQ_DEBUG_VALUE2_1_RD_DEBUG_VALUE1_2;
        end
        REQ_DEBUG_VALUE2_1_RD_DEBUG_VALUE1_2: begin
          addr <= params::ADDR_DEBUG_VALUE2_1;
          DEBUG_SETTINGS.VALUE[1][47:32] <= dout;
          state <= REQ_DEBUG_VALUE2_2_RD_DEBUG_VALUE1_3;
        end
        REQ_DEBUG_VALUE2_2_RD_DEBUG_VALUE1_3: begin
          addr <= params::ADDR_DEBUG_VALUE2_2;
          DEBUG_SETTINGS.VALUE[1][63:48] <= dout;
          state <= REQ_DEBUG_VALUE2_3_RD_DEBUG_VALUE2_0;
        end
        REQ_DEBUG_VALUE2_3_RD_DEBUG_VALUE2_0: begin
          addr <= params::ADDR_DEBUG_VALUE2_3;
          DEBUG_SETTINGS.VALUE[2][15:0] <= dout;
          state <= REQ_DEBUG_VALUE3_0_RD_DEBUG_VALUE2_1;
        end
        REQ_DEBUG_VALUE3_0_RD_DEBUG_VALUE2_1: begin
          addr <= params::ADDR_DEBUG_VALUE3_0;
          DEBUG_SETTINGS.VALUE[2][31:16] <= dout;
          state <= REQ_DEBUG_VALUE3_1_RD_DEBUG_VALUE2_2;
        end
        REQ_DEBUG_VALUE3_1_RD_DEBUG_VALUE2_2: begin
          addr <= params::ADDR_DEBUG_VALUE3_1;
          DEBUG_SETTINGS.VALUE[2][47:32] <= dout;
          state <= REQ_DEBUG_VALUE3_2_RD_DEBUG_VALUE2_3;
        end
        REQ_DEBUG_VALUE3_2_RD_DEBUG_VALUE2_3: begin
          addr <= params::ADDR_DEBUG_VALUE3_2;
          DEBUG_SETTINGS.VALUE[2][63:48] <= dout;
          state <= REQ_DEBUG_VALUE3_3_RD_DEBUG_VALUE3_0;
        end
        REQ_DEBUG_VALUE3_3_RD_DEBUG_VALUE3_0: begin
          addr <= params::ADDR_DEBUG_VALUE3_3;
          DEBUG_SETTINGS.VALUE[3][15:0] <= dout;
          state <= RD_DEBUG_VALUE3_1;
        end
        RD_DEBUG_VALUE3_1: begin
          DEBUG_SETTINGS.VALUE[3][31:16] <= dout;
          state <= RD_DEBUG_VALUE3_2;
        end
        RD_DEBUG_VALUE3_2: begin
          DEBUG_SETTINGS.VALUE[3][47:32] <= dout;
          state <= RD_DEBUG_VALUE3_3;
        end
        RD_DEBUG_VALUE3_3: begin
          DEBUG_SETTINGS.VALUE[3][63:48] <= dout;
          state <= DEBUG_COMMIT;
        end
        DEBUG_COMMIT: begin
          DEBUG_SETTINGS.UPDATE <= 1'b1;
          state <= DEBUG_CLR_UPDATE_SETTINGS_BIT;
        end
        DEBUG_CLR_UPDATE_SETTINGS_BIT: begin
          DEBUG_SETTINGS.UPDATE <= 1'b0;
          we <= 1'b1;
          addr <= params::ADDR_CTL_FLAG;
          din <= ctl_flags;
          ctl_flags_cand <= ctl_flags;
          state <= SET_DONE_0;
        end

        REQ_SYNC_TIME_0: begin
          we <= 1'b0;
          addr <= params::ADDR_SYNC_TIME_0;
          state <= REQ_SYNC_TIME_1;
        end
        REQ_SYNC_TIME_1: begin
          addr  <= params::ADDR_SYNC_TIME_1;
          state <= REQ_SYNC_TIME_2;
        end
        REQ_SYNC_TIME_2: begin
          addr  <= params::ADDR_SYNC_TIME_2;
          state <= REQ_SYNC_TIME_3_RD_SYNC_TIME_0;
        end
        REQ_SYNC_TIME_3_RD_SYNC_TIME_0: begin
          addr <= params::ADDR_SYNC_TIME_3;
          SYNC_SETTINGS.SYNC_TIME[15:0] <= dout;
          state <= RD_SYNC_TIME_1;
        end
        RD_SYNC_TIME_1: begin
          SYNC_SETTINGS.SYNC_TIME[31:16] <= dout;
          state <= RD_SYNC_TIME_2;
        end
        RD_SYNC_TIME_2: begin
          SYNC_SETTINGS.SYNC_TIME[47:32] <= dout;
          state <= RD_SYNC_TIME_3;
        end
        RD_SYNC_TIME_3: begin
          SYNC_SETTINGS.SYNC_TIME[63:48] <= dout;
          state <= SYNC_COMMIT;
        end
        SYNC_COMMIT: begin
          SYNC_SETTINGS.UPDATE <= 1'b1;
          state <= SYNC_CLR_UPDATE_SETTINGS_BIT;
        end
        SYNC_CLR_UPDATE_SETTINGS_BIT: begin
          SYNC_SETTINGS.UPDATE <= 1'b0;
          we <= 1'b1;
          addr <= params::ADDR_CTL_FLAG;
          din <= ctl_flags;
          ctl_flags_cand <= ctl_flags;
          state <= SET_DONE_0;
        end

        default: state <= WAIT_0;
      endcase
  end

  initial begin
    MOD_SETTINGS.UPDATE = 1'b0;
    MOD_SETTINGS.REQ_RD_BANK = 1'd0;
    MOD_SETTINGS.TRANSITION_MODE = params::TRANSITION_MODE_SYNC_IDX;
    MOD_SETTINGS.TRANSITION_VALUE = 64'd0;
    MOD_SETTINGS.CYCLE[0] = 16'd1;
    MOD_SETTINGS.CYCLE[1] = 16'd1;
    MOD_SETTINGS.FREQ_DIV[0] = 16'd10;
    MOD_SETTINGS.FREQ_DIV[1] = 16'd10;
    MOD_SETTINGS.REP[0] = 16'hFFFF;
    MOD_SETTINGS.REP[1] = 16'hFFFF;
    PATTERN_SETTINGS.UPDATE = 1'b0;
    PATTERN_SETTINGS.REQ_RD_BANK = 1'd0;
    PATTERN_SETTINGS.TRANSITION_MODE = params::TRANSITION_MODE_SYNC_IDX;
    PATTERN_SETTINGS.TRANSITION_VALUE = 64'd0;
    PATTERN_SETTINGS.MODE[0] = params::EMISSION_TYPE_RAW;
    PATTERN_SETTINGS.MODE[1] = params::EMISSION_TYPE_RAW;
    PATTERN_SETTINGS.CYCLE[0] = 16'd0;
    PATTERN_SETTINGS.CYCLE[1] = 16'd0;
    PATTERN_SETTINGS.FREQ_DIV[0] = 16'hFFFF;
    PATTERN_SETTINGS.FREQ_DIV[1] = 16'hFFFF;
    PATTERN_SETTINGS.SOUND_SPEED[0] = 16'd0;
    PATTERN_SETTINGS.SOUND_SPEED[1] = 16'd0;
    PATTERN_SETTINGS.REP[0] = 16'hFFFF;
    PATTERN_SETTINGS.REP[1] = 16'hFFFF;
    PATTERN_SETTINGS.NUM_FOCI[0] = 1;
    PATTERN_SETTINGS.NUM_FOCI[1] = 1;
    SILENCER_SETTINGS.UPDATE = 1'b0;
    SILENCER_SETTINGS.FLAG = 8'd0;
    SILENCER_SETTINGS.UPDATE_RATE_INTENSITY = 16'd256;
    SILENCER_SETTINGS.UPDATE_RATE_PHASE = 16'd256;
    SILENCER_SETTINGS.COMPLETION_STEPS_INTENSITY = 16'd10;
    SILENCER_SETTINGS.COMPLETION_STEPS_PHASE = 16'd40;
    DEBUG_SETTINGS.UPDATE = 1'b0;
    DEBUG_SETTINGS.VALUE[0] = {params::GPIO_O_TYPE_NONE, 56'd0};
    DEBUG_SETTINGS.VALUE[1] = {params::GPIO_O_TYPE_NONE, 56'd0};
    DEBUG_SETTINGS.VALUE[2] = {params::GPIO_O_TYPE_NONE, 56'd0};
    DEBUG_SETTINGS.VALUE[3] = {params::GPIO_O_TYPE_NONE, 56'd0};
    SYNC_SETTINGS.UPDATE = 1'b0;
    SYNC_SETTINGS.SYNC_TIME = 64'd0;
  end

endmodule
`default_nettype wire
