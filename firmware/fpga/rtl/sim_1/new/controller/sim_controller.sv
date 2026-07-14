`timescale 1ns / 1ps
module sim_controller ();

  `include "define.vh"

  localparam int DEPTH = 249;

  logic CLK;
  logic locked;

  sim_helper_random sim_helper_random ();
  sim_helper_bram #(.DEPTH(DEPTH)) sim_helper_bram ();

  cnt_bus_if cnt_bus ();
  phase_corr_bus_if phase_corr_bus ();
  modulation_bus_if mod_bus ();
  emission_bus_if emission_bus ();
  pwe_table_bus_if pwe_table_bus ();
  output_mask_bus_if output_mask_bus ();

  memory memory (
      .CLK(CLK),
      .MEM_BUS(sim_helper_bram.memory_bus.bram_port),
      .CNT_BUS(cnt_bus.in_port),
      .PHASE_CORR_BUS(phase_corr_bus.in_port),
      .OUTPUT_MASK_BUS(output_mask_bus.in_port),
      .MOD_BUS(mod_bus.in_port),
      .EMISSION_BUS(emission_bus.in_port),
      .PWE_TABLE_BUS(pwe_table_bus.in_port)
  );

  sim_helper_clk sim_helper_clk (
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME()
  );

  logic thermo;
  logic pattern_bank;
  logic mod_bank;
  logic [15:0] pattern_cycle;
  logic pattern_stopped;
  logic mod_stopped;
  logic transition_pending;
  logic gpio_in[4];
  settings::mod_settings_t mod_settings;
  settings::pattern_settings_t pattern_settings;
  settings::silencer_settings_t silencer_settings;
  settings::sync_settings_t sync_settings;
  settings::debug_settings_t debug_settings;
  logic FORCE_FAN;

  controller controller (
      .CLK(CLK),
      .THERMO(thermo),
      .PATTERN_BANK(pattern_bank),
      .MOD_BANK(mod_bank),
      .PATTERN_CYCLE(pattern_cycle),
      .PATTERN_STOPPED(pattern_stopped),
      .MOD_STOPPED(mod_stopped),
      .TRANSITION_PENDING(transition_pending),
      .cnt_bus(cnt_bus.out_port),
      .MOD_SETTINGS(mod_settings),
      .PATTERN_SETTINGS(pattern_settings),
      .SILENCER_SETTINGS(silencer_settings),
      .SYNC_SETTINGS(sync_settings),
      .DEBUG_SETTINGS(debug_settings),
      .FORCE_FAN(FORCE_FAN),
      .GPIO_IN(gpio_in)
  );

  settings::mod_settings_t mod_settings_in;
  settings::pattern_settings_t pattern_settings_in;
  settings::silencer_settings_t silencer_settings_in;
  settings::sync_settings_t sync_settings_in;
  settings::debug_settings_t debug_settings_in;

  logic [15:0] fpga_state;

  initial begin

    thermo = 1'b1;
    mod_bank = 1'b1;
    pattern_bank = 1'b0;
    pattern_cycle = 16'd5;
    pattern_stopped = 1'b1;
    mod_stopped = 1'b0;
    transition_pending = 1'b1;

    mod_settings_in.UPDATE = 1'b1;
    mod_settings_in.REQ_RD_BANK = sim_helper_random.range(1'b1, 0);
    mod_settings_in.TRANSITION_MODE = sim_helper_random.range(8'hFF, 0);
    mod_settings_in.TRANSITION_VALUE = sim_helper_random.range(64'hFFFFFFFFFFFFFFFF, 0);
    mod_settings_in.CYCLE[0] = sim_helper_random.range(16'hFFFF, 0);
    mod_settings_in.CYCLE[1] = sim_helper_random.range(16'hFFFF, 0);
    mod_settings_in.FREQ_DIV[0] = sim_helper_random.range(16'hFFFF, 0);
    mod_settings_in.FREQ_DIV[1] = sim_helper_random.range(16'hFFFF, 0);
    mod_settings_in.REP[0] = sim_helper_random.range(16'hFFFF, 0);
    mod_settings_in.REP[1] = sim_helper_random.range(16'hFFFF, 0);

    pattern_settings_in.UPDATE = 1'b1;
    pattern_settings_in.REQ_RD_BANK = sim_helper_random.range(1'b1, 0);
    pattern_settings_in.TRANSITION_MODE = sim_helper_random.range(8'hFF, 0);
    pattern_settings_in.TRANSITION_VALUE = sim_helper_random.range(64'hFFFFFFFFFFFFFFFF, 0);
    pattern_settings_in.MODE[0] = sim_helper_random.range(1'b1, 0);
    pattern_settings_in.MODE[1] = sim_helper_random.range(1'b1, 0);
    pattern_settings_in.CYCLE[0] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.CYCLE[1] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.FREQ_DIV[0] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.FREQ_DIV[1] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.REP[0] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.REP[1] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.SOUND_SPEED[0] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.SOUND_SPEED[1] = sim_helper_random.range(16'hFFFF, 0);
    pattern_settings_in.NUM_FOCI[0] = sim_helper_random.range(8'd8, 0);
    pattern_settings_in.NUM_FOCI[1] = sim_helper_random.range(8'd8, 0);

    silencer_settings_in.UPDATE = 1'b1;
    silencer_settings_in.FLAG = sim_helper_random.range(8'hFF, 0);
    silencer_settings_in.UPDATE_RATE_INTENSITY = sim_helper_random.range(8'hFF, 0);
    silencer_settings_in.UPDATE_RATE_PHASE = sim_helper_random.range(8'hFF, 0);
    silencer_settings_in.COMPLETION_STEPS_INTENSITY = sim_helper_random.range(8'hFF, 0);
    silencer_settings_in.COMPLETION_STEPS_PHASE = sim_helper_random.range(8'hFF, 0);

    sync_settings_in.UPDATE = 1'b1;
    sync_settings_in.ECAT_SYNC_TIME = sim_helper_random.range(64'hFFFFFFFFFFFFFFFF, 0);
    sync_settings_in.ECAT_SYNC_CYCLE = sim_helper_random.range(32'hFFFFFFFF, 0);

    debug_settings_in.UPDATE = 1'b1;
    debug_settings_in.VALUE[0] = sim_helper_random.range(64'hFFFF, 0);
    debug_settings_in.VALUE[1] = sim_helper_random.range(64'hFFFF, 0);
    debug_settings_in.VALUE[2] = sim_helper_random.range(64'hFFFF, 0);
    debug_settings_in.VALUE[3] = sim_helper_random.range(64'hFFFF, 0);

    @(posedge locked);

    sim_helper_bram.write_mod_settings(mod_settings_in);
    sim_helper_bram.write_pattern_settings(pattern_settings_in);
    sim_helper_bram.write_silencer_settings(silencer_settings_in);
    sim_helper_bram.write_sync_settings(sync_settings_in);
    sim_helper_bram.write_debug_settings(debug_settings_in);
    $display("memory initialized");

    sim_helper_bram.bram_write(params::BRAM_SELECT_CONTROLLER, params::ADDR_CTL_FLAG,
                               (16'd1 << params::CTL_FLAG_BIT_MOD_SET)
                               | (16'd1 << params::CTL_FLAG_BIT_PATTERN_SET)
                               | (16'd1 << params::CTL_FLAG_BIT_SILENCER_SET)
                               | (16'd1 << params::CTL_FLAG_BIT_DEBUG_SET)
                               | (16'd1 << params::CTL_FLAG_BIT_SYNC_SET));
    @(posedge mod_settings.UPDATE);
    `ASSERT_EQ(mod_settings_in, mod_settings);

    @(posedge pattern_settings.UPDATE);
    `ASSERT_EQ(pattern_settings_in, pattern_settings);

    @(posedge silencer_settings.UPDATE);
    `ASSERT_EQ(silencer_settings_in, silencer_settings);

    @(posedge debug_settings.UPDATE);
    `ASSERT_EQ(debug_settings_in, debug_settings);

    @(posedge sync_settings.UPDATE);
    `ASSERT_EQ(sync_settings_in, sync_settings);

    sim_helper_bram.read_cnt(params::ADDR_FPGA_STATE, fpga_state);
    `ASSERT_EQ(
        {8'h00, 1'h0, transition_pending, mod_stopped, pattern_stopped, pattern_cycle == '0, pattern_bank, mod_bank, thermo},
        fpga_state);

    $display("OK! sim_controller");
    $finish();
  end

endmodule
