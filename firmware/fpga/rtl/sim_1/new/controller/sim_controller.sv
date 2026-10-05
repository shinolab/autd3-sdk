`timescale 1ns / 1ps
module sim_controller ();

  `include "define.vh"

  localparam int DEPTH = 249;

  localparam bit [15:0] PersistentFlags = (16'd1 << params::CTL_FLAG_BIT_FORCE_FAN)
      | (16'd1 << params::CTL_FLAG_BIT_GPIO_IN_1) | (16'd1 << params::CTL_FLAG_BIT_GPIO_IN_3);

  logic CLK;
  logic locked;
  logic enable_gate;
  logic enable;
  logic enable_d = 1'b0;

  assign enable = locked & enable_gate;

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
  logic pattern_ext_active;
  logic mod_ext_active;
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
      .ENABLE(enable),
      .THERMO(thermo),
      .PATTERN_BANK(pattern_bank),
      .MOD_BANK(mod_bank),
      .PATTERN_EXT_ACTIVE(pattern_ext_active),
      .MOD_EXT_ACTIVE(mod_ext_active),
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

  always_ff @(posedge CLK) enable_d <= enable;

  always @(posedge CLK) begin
    if (!enable && !enable_d) begin
      `ASSERT_EQ(1'b0, cnt_bus.WE);
    end
  end

  logic idle_check;
  logic expected_force_fan;
  logic expected_gpio_in[4];

  always @(posedge CLK) begin
    if (idle_check === 1'b1) begin
      `ASSERT_EQ(1'b0, mod_settings.UPDATE);
      `ASSERT_EQ(1'b0, pattern_settings.UPDATE);
      `ASSERT_EQ(1'b0, silencer_settings.UPDATE);
      `ASSERT_EQ(1'b0, debug_settings.UPDATE);
      `ASSERT_EQ(1'b0, sync_settings.UPDATE);
      `ASSERT_EQ(expected_force_fan, FORCE_FAN);
      `ASSERT_EQ(expected_gpio_in[0], gpio_in[0]);
      `ASSERT_EQ(expected_gpio_in[1], gpio_in[1]);
      `ASSERT_EQ(expected_gpio_in[2], gpio_in[2]);
      `ASSERT_EQ(expected_gpio_in[3], gpio_in[3]);
    end
  end

  logic [15:0] glitch_value;

  task automatic inject_ctl_flag_glitch(input logic [15:0] value);
    @(posedge CLK);
    #1;
    glitch_value = value;
    force cnt_bus.DOUT = glitch_value;
    @(posedge CLK);
    #1;
    release cnt_bus.DOUT;
  endtask

  task automatic assert_persistent_flags();
    `ASSERT_EQ(1'b1, FORCE_FAN);
    `ASSERT_EQ(1'b0, gpio_in[0]);
    `ASSERT_EQ(1'b1, gpio_in[1]);
    `ASSERT_EQ(1'b0, gpio_in[2]);
    `ASSERT_EQ(1'b1, gpio_in[3]);
  endtask

  settings::mod_settings_t mod_settings_in;
  settings::pattern_settings_t pattern_settings_in;
  settings::silencer_settings_t silencer_settings_in;
  settings::sync_settings_t sync_settings_in;
  settings::debug_settings_t debug_settings_in;

  logic [15:0] fpga_state;
  logic [15:0] ctl_flag;

  localparam bit [7:0] TransitionImmediate = 8'hFF;
  localparam bit [7:0] SilencerStrict = 8'd1 << params::SILENCER_FLAG_BIT_STRICT_MODE;
  localparam bit [7:0] SilencerFixedUpdateRate = 8'd1 << params::SILENCER_FLAG_BIT_FIXED_UPDATE_RATE_MODE;

  int mod_updates = 0;
  int pattern_updates = 0;
  int silencer_updates = 0;

  always @(posedge CLK) begin
    if (mod_settings.UPDATE) mod_updates++;
    if (pattern_settings.UPDATE) pattern_updates++;
    if (silencer_settings.UPDATE) silencer_updates++;
  end

  task automatic latch(input int flag_bit, output logic rejected);
    logic [15:0] value;
    sim_helper_bram.write_cnt(params::ADDR_CTL_FLAG, PersistentFlags | (16'd1 << flag_bit));
    do begin
      sim_helper_bram.read_cnt(params::ADDR_CTL_FLAG, value);
    end while (value[flag_bit]);
    sim_helper_bram.read_cnt(params::ADDR_SILENCER_SET_RESULT, value);
    rejected = value[flag_bit];
    repeat (8) @(posedge CLK);
  endtask

  task automatic latch_mod(input logic req, input logic [7:0] mode, input logic [15:0] div0, input logic [15:0] div1, input logic [15:0] rep0,
                           input logic [15:0] rep1, input logic expect_rejected);
    settings::mod_settings_t previous;
    int updates;
    logic rejected;
    previous = mod_settings;
    updates = mod_updates;
    mod_settings_in.UPDATE = 1'b0;
    mod_settings_in.REQ_RD_BANK = req;
    mod_settings_in.TRANSITION_MODE = mode;
    mod_settings_in.FREQ_DIV[0] = div0;
    mod_settings_in.FREQ_DIV[1] = div1;
    mod_settings_in.REP[0] = rep0;
    mod_settings_in.REP[1] = rep1;
    sim_helper_bram.write_mod_settings(mod_settings_in);
    latch(params::CTL_FLAG_BIT_MOD_SET, rejected);
    `ASSERT_EQ(expect_rejected, rejected);
    if (expect_rejected) begin
      `ASSERT_EQ(previous, mod_settings);
      `ASSERT_EQ(updates, mod_updates);
    end else begin
      `ASSERT_EQ(mod_settings_in, mod_settings);
      `ASSERT_EQ(updates + 1, mod_updates);
    end
  endtask

  task automatic latch_pattern(input logic req, input logic [7:0] mode, input logic [15:0] div0, input logic [15:0] div1, input logic [15:0] rep0,
                               input logic [15:0] rep1, input logic expect_rejected);
    settings::pattern_settings_t previous;
    int updates;
    logic rejected;
    previous = pattern_settings;
    updates = pattern_updates;
    pattern_settings_in.UPDATE = 1'b0;
    pattern_settings_in.REQ_RD_BANK = req;
    pattern_settings_in.TRANSITION_MODE = mode;
    pattern_settings_in.FREQ_DIV[0] = div0;
    pattern_settings_in.FREQ_DIV[1] = div1;
    pattern_settings_in.REP[0] = rep0;
    pattern_settings_in.REP[1] = rep1;
    sim_helper_bram.write_pattern_settings(pattern_settings_in);
    latch(params::CTL_FLAG_BIT_PATTERN_SET, rejected);
    `ASSERT_EQ(expect_rejected, rejected);
    if (expect_rejected) begin
      `ASSERT_EQ(previous, pattern_settings);
      `ASSERT_EQ(updates, pattern_updates);
    end else begin
      `ASSERT_EQ(pattern_settings_in, pattern_settings);
      `ASSERT_EQ(updates + 1, pattern_updates);
    end
  endtask

  task automatic latch_silencer(input logic [7:0] flag, input logic [15:0] steps_intensity, input logic [15:0] steps_phase,
                                input logic expect_rejected);
    settings::silencer_settings_t previous;
    int updates;
    logic rejected;
    previous = silencer_settings;
    updates = silencer_updates;
    silencer_settings_in.UPDATE = 1'b0;
    silencer_settings_in.FLAG = flag;
    silencer_settings_in.COMPLETION_STEPS_INTENSITY = steps_intensity;
    silencer_settings_in.COMPLETION_STEPS_PHASE = steps_phase;
    sim_helper_bram.write_silencer_settings(silencer_settings_in);
    latch(params::CTL_FLAG_BIT_SILENCER_SET, rejected);
    `ASSERT_EQ(expect_rejected, rejected);
    if (expect_rejected) begin
      `ASSERT_EQ(previous, silencer_settings);
      `ASSERT_EQ(updates, silencer_updates);
    end else begin
      `ASSERT_EQ(silencer_settings_in, silencer_settings);
      `ASSERT_EQ(updates + 1, silencer_updates);
    end
  endtask

  initial begin

    enable_gate = 1'b1;
    idle_check = 1'b0;
    expected_force_fan = 1'b0;
    expected_gpio_in = '{1'b0, 1'b0, 1'b0, 1'b0};

    thermo = 1'b1;
    mod_bank = 1'b1;
    pattern_bank = 1'b0;
    mod_ext_active = 1'b0;
    pattern_ext_active = 1'b0;
    pattern_cycle = 16'd5;
    pattern_stopped = 1'b1;
    mod_stopped = 1'b0;
    transition_pending = 1'b1;
    sync_resync_count = 8'd7;

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
    silencer_settings_in.FLAG = sim_helper_random.range(8'hFF, 0) & ~(8'd1 << params::SILENCER_FLAG_BIT_STRICT_MODE);
    silencer_settings_in.UPDATE_RATE_INTENSITY = sim_helper_random.range(8'hFF, 0);
    silencer_settings_in.UPDATE_RATE_PHASE = sim_helper_random.range(8'hFF, 0);
    silencer_settings_in.COMPLETION_STEPS_INTENSITY = sim_helper_random.range(8'hFF, 0);
    silencer_settings_in.COMPLETION_STEPS_PHASE = sim_helper_random.range(8'hFF, 0);

    sync_settings_in.UPDATE = 1'b1;
    sync_settings_in.SYNC_TIME = sim_helper_random.range(64'hFFFFFFFFFFFFFFFF, 0);

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
    `ASSERT_EQ({sync_resync_count, 1'h0, transition_pending, mod_stopped, pattern_stopped, pattern_cycle == '0, pattern_bank, mod_bank, thermo},
               fpga_state);

    repeat (32) @(posedge CLK);
    sim_helper_bram.read_cnt(params::ADDR_CTL_FLAG, ctl_flag);
    `ASSERT_EQ(16'd0, ctl_flag);

    sim_helper_bram.write_cnt(params::ADDR_CTL_FLAG, PersistentFlags | (16'd1 << params::CTL_FLAG_BIT_MOD_SET));
    @(posedge mod_settings.UPDATE);
    repeat (32) @(posedge CLK);
    assert_persistent_flags();
    sim_helper_bram.read_cnt(params::ADDR_CTL_FLAG, ctl_flag);
    `ASSERT_EQ(PersistentFlags, ctl_flag);
    $display("OK! persistent CTL_FLAG bits survive a latch sequence");

    @(negedge CLK);
    expected_force_fan = 1'b1;
    expected_gpio_in = '{1'b0, 1'b1, 1'b0, 1'b1};
    idle_check = 1'b1;
    for (int i = 0; i < 4; i++) begin
      repeat (i) @(posedge CLK);
      inject_ctl_flag_glitch(16'hFFFF);
      repeat (64) @(posedge CLK);
      inject_ctl_flag_glitch(16'h0000);
      repeat (64) @(posedge CLK);
    end
    @(negedge CLK);
    idle_check = 1'b0;
    sim_helper_bram.read_cnt(params::ADDR_CTL_FLAG, ctl_flag);
    `ASSERT_EQ(PersistentFlags, ctl_flag);
    $display("OK! a corrupted CTL_FLAG read is rejected");

    for (int i = 0; i < 8; i++) begin
      sim_helper_bram.write_cnt(params::ADDR_CTL_FLAG, PersistentFlags | (16'd1 << params::CTL_FLAG_BIT_MOD_SET));
      @(posedge mod_settings.UPDATE);
      `ASSERT_EQ(mod_settings_in, mod_settings);
      @(negedge mod_settings.UPDATE);
      repeat (i) @(posedge CLK);
      inject_ctl_flag_glitch(PersistentFlags | (16'd1 << params::CTL_FLAG_BIT_MOD_SET));
      @(negedge CLK);
      idle_check = 1'b1;
      repeat (64) @(posedge CLK);
      @(negedge CLK);
      idle_check = 1'b0;
    end
    sim_helper_bram.read_cnt(params::ADDR_CTL_FLAG, ctl_flag);
    `ASSERT_EQ(PersistentFlags, ctl_flag);
    $display("OK! a stale candidate cannot re-arm the sequence just cleared");

    sim_helper_bram.write_cnt(params::ADDR_CTL_FLAG, PersistentFlags | (16'd1 << params::CTL_FLAG_BIT_PATTERN_SET));
    wait (controller.ctl_flags[params::CTL_FLAG_BIT_PATTERN_SET] === 1'b1);
    repeat (6) @(posedge CLK);
    `ASSERT_EQ(1'b0, pattern_settings.UPDATE);
    enable_gate = 1'b0;
    repeat (2) @(posedge CLK);
    `ASSERT_EQ(16'd0, controller.ctl_flags);
    `ASSERT_EQ(1'b0, pattern_settings.UPDATE);
    `ASSERT_EQ(1'b0, FORCE_FAN);
    repeat (8) @(posedge CLK);
    enable_gate = 1'b1;
    @(posedge pattern_settings.UPDATE);
    `ASSERT_EQ(pattern_settings_in, pattern_settings);
    repeat (32) @(posedge CLK);
    assert_persistent_flags();
    $display("OK! losing ENABLE mid-sequence replays the latch after relock");

    sim_helper_bram.write_cnt(params::ADDR_CTL_FLAG, PersistentFlags | (16'd1 << params::CTL_FLAG_BIT_PATTERN_SET));
    @(posedge pattern_settings.UPDATE);
    enable_gate = 1'b0;
    repeat (2) @(posedge CLK);
    `ASSERT_EQ(16'd0, controller.ctl_flags);
    `ASSERT_EQ(1'b0, mod_settings.UPDATE);
    `ASSERT_EQ(1'b0, pattern_settings.UPDATE);
    `ASSERT_EQ(1'b0, silencer_settings.UPDATE);
    `ASSERT_EQ(1'b0, debug_settings.UPDATE);
    `ASSERT_EQ(1'b0, sync_settings.UPDATE);
    `ASSERT_EQ(1'b0, FORCE_FAN);
    `ASSERT_EQ(1'b0, gpio_in[1]);
    `ASSERT_EQ(1'b0, gpio_in[3]);

    repeat (8) @(posedge CLK);
    enable_gate = 1'b1;
    repeat (128) @(posedge CLK);
    assert_persistent_flags();
    $display("OK! losing ENABLE clears ctl_flags and every UPDATE");

    sim_helper_bram.write_cnt(params::ADDR_CTL_FLAG, PersistentFlags | (16'd1 << params::CTL_FLAG_BIT_PATTERN_SET));
    @(posedge pattern_settings.UPDATE);
    `ASSERT_EQ(pattern_settings_in, pattern_settings);
    repeat (32) @(posedge CLK);
    assert_persistent_flags();
    sim_helper_bram.read_cnt(params::ADDR_CTL_FLAG, ctl_flag);
    `ASSERT_EQ(PersistentFlags, ctl_flag);

    mod_bank = 1'b0;
    pattern_bank = 1'b0;
    latch_silencer(8'd0, 16'd8, 16'd8, 1'b0);
    latch_pattern(1'b0, TransitionImmediate, 16'd100, 16'd100, 16'hFFFF, 16'hFFFF, 1'b0);

    latch_mod(1'b0, TransitionImmediate, 16'd5, 16'd100, 16'hFFFF, 16'd0, 1'b0);
    latch_mod(1'b1, params::TRANSITION_MODE_SYS_TIME, 16'd5, 16'd100, 16'hFFFF, 16'd0, 1'b0);
    latch_silencer(SilencerStrict, 16'd8, 16'd8, 1'b1);
    $display("OK! strict is rejected while a faster bank still plays ahead of a pending transition");

    latch_mod(1'b0, params::TRANSITION_MODE_EXT, 16'd100, 16'd5, 16'hFFFF, 16'hFFFF, 1'b0);
    latch_silencer(SilencerStrict, 16'd8, 16'd8, 1'b1);
    mod_bank = 1'b1;
    latch_silencer(SilencerStrict, 16'd8, 16'd8, 1'b1);
    mod_bank = 1'b0;
    $display("OK! strict is rejected while EXT alternates onto a faster bank");

    mod_ext_active = 1'b1;
    latch_mod(1'b0, params::TRANSITION_MODE_SYNC_IDX, 16'd100, 16'd5, 16'd0, 16'hFFFF, 1'b0);
    latch_silencer(SilencerStrict, 16'd8, 16'd8, 1'b1);
    mod_ext_active = 1'b0;
    $display("OK! strict is rejected while EXT keeps alternating under a finite request to the playing bank");

    latch_mod(1'b0, TransitionImmediate, 16'd100, 16'd5, 16'hFFFF, 16'hFFFF, 1'b0);
    latch_silencer(SilencerStrict, 16'd8, 16'd8, 1'b0);
    latch_mod(1'b0, TransitionImmediate, 16'd100, 16'd3, 16'hFFFF, 16'hFFFF, 1'b0);
    $display("OK! a faster divider on an unused bank does not trip the guard");

    latch_mod(1'b1, TransitionImmediate, 16'd100, 16'd3, 16'hFFFF, 16'hFFFF, 1'b1);
    latch_mod(1'b0, params::TRANSITION_MODE_EXT, 16'd100, 16'd3, 16'hFFFF, 16'hFFFF, 1'b1);
    latch_mod(1'b0, TransitionImmediate, 16'd7, 16'd100, 16'hFFFF, 16'hFFFF, 1'b1);
    latch_mod(1'b0, TransitionImmediate, 16'd8, 16'd3, 16'hFFFF, 16'hFFFF, 1'b0);
    $display("OK! a strict violation on the requested bank is rejected and leaves MOD_SETTINGS untouched");

    latch_mod(1'b1, params::TRANSITION_MODE_SYS_TIME, 16'd3, 16'd100, 16'hFFFF, 16'd0, 1'b1);
    latch_mod(1'b1, params::TRANSITION_MODE_SYS_TIME, 16'd8, 16'd100, 16'hFFFF, 16'd0, 1'b0);
    latch_mod(1'b1, TransitionImmediate, 16'd3, 16'd100, 16'hFFFF, 16'hFFFF, 1'b0);
    latch_silencer(SilencerStrict, 16'd8, 16'd8, 1'b0);
    mod_bank = 1'b1;
    $display("OK! a finite request keeps the playing bank under the guard, an immediate one releases it");

    latch_silencer(SilencerStrict, 16'd8, 16'd40, 1'b0);
    latch_pattern(1'b0, TransitionImmediate, 16'd39, 16'd100, 16'hFFFF, 16'hFFFF, 1'b1);
    latch_pattern(1'b0, TransitionImmediate, 16'd40, 16'd7, 16'hFFFF, 16'hFFFF, 1'b0);
    latch_silencer(SilencerStrict, 16'd8, 16'd41, 1'b1);
    latch_silencer(SilencerStrict, 16'd41, 16'd8, 1'b1);
    $display("OK! PATTERN is guarded on both intensity and phase");

    latch_silencer(SilencerStrict | SilencerFixedUpdateRate, 16'd200, 16'd200, 1'b0);
    latch_mod(1'b1, TransitionImmediate, 16'd3, 16'd1, 16'hFFFF, 16'hFFFF, 1'b0);
    latch_pattern(1'b0, TransitionImmediate, 16'd1, 16'd1, 16'hFFFF, 16'hFFFF, 1'b0);
    latch_silencer(SilencerStrict, 16'd2, 16'd2, 1'b1);
    latch_silencer(8'd0, 16'd200, 16'd200, 1'b0);
    $display("OK! fixed update rate mode releases the guard");

    $display("OK! sim_controller");
    $finish();
  end

endmodule
