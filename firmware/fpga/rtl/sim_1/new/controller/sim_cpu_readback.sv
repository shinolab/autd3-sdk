`timescale 1ns / 1ps
module sim_cpu_readback ();

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
  flash_bus_if flash_bus ();
  output_mask_bus_if output_mask_bus ();

  memory memory (
      .CLK(CLK),
      .MEM_BUS(sim_helper_bram.memory_bus.bram_port),
      .CNT_BUS(cnt_bus.in_port),
      .PHASE_CORR_BUS(phase_corr_bus.in_port),
      .OUTPUT_MASK_BUS(output_mask_bus.in_port),
      .MOD_BUS(mod_bus.in_port),
      .EMISSION_BUS(emission_bus.in_port),
      .PWE_TABLE_BUS(pwe_table_bus.in_port),
      .FLASH_BUS(flash_bus.host_port)
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
  logic [7:0] sync_resync_count;
  logic gpio_in[4];
  settings::mod_settings_t mod_settings;
  settings::pattern_settings_t pattern_settings;
  settings::silencer_settings_t silencer_settings;
  settings::sync_settings_t sync_settings;
  settings::debug_settings_t debug_settings;
  logic FORCE_FAN;

  controller controller (
      .CLK(CLK),
      .ENABLE(locked),
      .THERMO(thermo),
      .PATTERN_BANK(pattern_bank),
      .MOD_BANK(mod_bank),
      .PATTERN_CYCLE(pattern_cycle),
      .PATTERN_STOPPED(pattern_stopped),
      .MOD_STOPPED(mod_stopped),
      .TRANSITION_PENDING(transition_pending),
      .SYNC_RESYNC_COUNT(sync_resync_count),
      .cnt_bus(cnt_bus.out_port),
      .MOD_SETTINGS(mod_settings),
      .PATTERN_SETTINGS(pattern_settings),
      .SILENCER_SETTINGS(silencer_settings),
      .SYNC_SETTINGS(sync_settings),
      .DEBUG_SETTINGS(debug_settings),
      .FORCE_FAN(FORCE_FAN),
      .GPIO_IN(gpio_in)
  );

  logic [15:0] value;

  localparam bit [15:0] PersistentFlags = (16'd1 << params::CTL_FLAG_BIT_FORCE_FAN)
      | (16'd1 << params::CTL_FLAG_BIT_GPIO_IN_0) | (16'd1 << params::CTL_FLAG_BIT_GPIO_IN_2);

  initial begin

    thermo = 1'b1;
    mod_bank = 1'b0;
    pattern_bank = 1'b1;
    pattern_cycle = 16'd0;
    pattern_stopped = 1'b0;
    mod_stopped = 1'b1;
    transition_pending = 1'b0;
    sync_resync_count = 8'd3;

    @(posedge locked);

    // let the controller boot FSM write the version registers and the initial FPGA_STATE
    repeat (64) @(posedge CLK);

    sim_helper_bram.read_cnt(params::ADDR_VERSION_NUM_MAJOR, value);
    `ASSERT_EQ({8'd1 << params::FuncFlashOtaBit, params::VersionNumMajor}, value);

    sim_helper_bram.read_cnt(params::ADDR_VERSION_NUM_MINOR, value);
    `ASSERT_EQ({8'h00, params::VersionNumMinor}, value);

    sim_helper_bram.read_cnt(params::ADDR_VERSION_NUM_PATCH, value);
    `ASSERT_EQ({8'h00, params::VersionNumPatch}, value);

    sim_helper_bram.read_cnt(params::ADDR_FPGA_STATE, value);
    `ASSERT_EQ({sync_resync_count, 1'h0, transition_pending, mod_stopped, pattern_stopped, pattern_cycle == '0, pattern_bank, mod_bank, thermo},
               value);

    sim_helper_bram.write_cnt(params::ADDR_CTL_FLAG, PersistentFlags);
    repeat (32) @(posedge CLK);
    `ASSERT_EQ(1'b1, FORCE_FAN);
    `ASSERT_EQ(1'b1, gpio_in[0]);
    `ASSERT_EQ(1'b0, gpio_in[1]);
    `ASSERT_EQ(1'b1, gpio_in[2]);
    `ASSERT_EQ(1'b0, gpio_in[3]);

    sim_helper_bram.read_cnt(params::ADDR_CTL_FLAG, value);
    `ASSERT_EQ(PersistentFlags, value);

    $display("OK! sim_cpu_readback");
    $finish();
  end

endmodule
