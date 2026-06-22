`timescale 1ns / 1ps
module sim_output_mask ();

  `include "define.vh"

  logic CLK;
  logic locked;
  logic [56:0] SYS_TIME;

  localparam int DEPTH = 249;
  localparam int SIZE = 16;

  sim_helper_bram sim_helper_bram ();
  sim_helper_random sim_helper_random ();

  settings::pattern_settings_t pattern_settings;

  logic [7:0] intensity;
  logic [7:0] phase;
  logic [12:0] debug_idx;
  logic debug_bank;
  logic dout_valid;

  logic [13:0] cycle_buf[params::NumBanks];
  logic [15:0] freq_div_buf[params::NumBanks];
  logic [7:0] intensity_buf[params::NumBanks][SIZE][DEPTH];
  logic [7:0] phase_buf[params::NumBanks][SIZE][DEPTH];

  logic [255:0] output_mask_buf[params::NumBanks];

  cnt_bus_if cnt_bus ();
  phase_corr_bus_if phase_corr_bus ();
  modulation_bus_if mod_bus ();
  emission_bus_if emission_bus ();
  pwe_table_bus_if pwe_table_bus ();
  output_mask_bus_if output_mask_bus ();

  memory memory (
      .CLK(CLK),
      .MRCC_25P6M(MRCC_25P6M),
      .MEM_BUS(sim_helper_bram.memory_bus.bram_port),
      .CNT_BUS(cnt_bus.in_port),
      .PHASE_CORR_BUS(phase_corr_bus.in_port),
      .OUTPUT_MASK_BUS(output_mask_bus.in_port),
      .MOD_BUS(mod_bus.in_port),
      .EMISSION_BUS(emission_bus.in_port),
      .PWE_TABLE_BUS(pwe_table_bus.in_port)
  );

  sim_helper_clk sim_helper_clk (
      .MRCC_25P6M(MRCC_25P6M),
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME(SYS_TIME)
  );

  time_cnt_generator #(
      .DEPTH(DEPTH)
  ) time_cnt_generator (
      .CLK(CLK),
      .SYS_TIME(SYS_TIME),
      .SKIP_ONE_ASSERT(1'b0),
      .TIME_CNT(),
      .UPDATE(UPDATE)
  );

  emission #(
      .DEPTH(DEPTH)
  ) emission (
      .CLK(CLK),
      .SYS_TIME(SYS_TIME),
      .UPDATE(UPDATE),
      .PATTERN_SETTINGS(pattern_settings),
      .EMISSION_BUS(emission_bus.emission_port),
      .EMISSION_BUS_FOCUS(emission_bus.out_focus_port),
      .EMISSION_BUS_RAW(emission_bus.out_raw_port),
      .OUTPUT_MASK_BUS(output_mask_bus.out_port),
      .INTENSITY(intensity),
      .PHASE(phase),
      .DOUT_VALID(dout_valid),
      .DEBUG_IDX(debug_idx),
      .DEBUG_BANK(debug_bank)
  );

  task automatic update(input logic req_bank, input logic [15:0] rep);
    @(posedge CLK);
    pattern_settings.UPDATE <= 1'b1;
    pattern_settings.REQ_RD_BANK <= req_bank;
    pattern_settings.REP[req_bank] <= rep;
    pattern_settings.CYCLE[req_bank] <= cycle_buf[req_bank] - 1;
    pattern_settings.FREQ_DIV[req_bank] <= freq_div_buf[req_bank];
    @(posedge CLK);
    pattern_settings.UPDATE <= 1'b0;
  endtask

  task automatic wait_bank(input logic bank);
    while (1) begin
      @(posedge CLK);
      if (debug_bank === bank) begin
        break;
      end
    end
  endtask

  task automatic check(input logic bank);
    while (1) begin
      @(posedge CLK);
      if (~dout_valid) begin
        break;
      end
    end

    for (int j = 0; j < cycle_buf[bank] * freq_div_buf[bank]; j++) begin
      while (1) begin
        @(posedge CLK);
        if (dout_valid) begin
          break;
        end
      end
      $display("check %d/%d", j + 1, cycle_buf[bank]);
      for (int i = 0; i < DEPTH; i++) begin
        if (output_mask_buf[bank][i]) begin
          `ASSERT_EQ(intensity_buf[bank][debug_idx][i], intensity);
        end else begin
          `ASSERT_EQ(8'h00, intensity);
        end
        `ASSERT_EQ(phase_buf[bank][debug_idx][i], phase);
        @(posedge CLK);
      end
    end
  endtask

  initial begin
    sim_helper_random.init();

    cycle_buf[0] = SIZE;
    cycle_buf[1] = SIZE / 4;
    freq_div_buf[0] = 1;
    freq_div_buf[1] = 3;

    pattern_settings.UPDATE = 0;
    pattern_settings.TRANSITION_MODE = params::TRANSITION_MODE_SYNC_IDX;
    pattern_settings.TRANSITION_VALUE = 0;
    pattern_settings.MODE[0] = params::EMISSION_TYPE_RAW;
    pattern_settings.MODE[1] = params::EMISSION_TYPE_RAW;
    pattern_settings.CYCLE[0] = '0;
    pattern_settings.FREQ_DIV[0] = '1;
    pattern_settings.CYCLE[1] = '0;
    pattern_settings.FREQ_DIV[1] = '1;

    @(posedge locked);

    for (int bank = 0; bank < params::NumBanks; bank++) begin
      for (int i = 0; i < SIZE; i++) begin
        for (int j = 0; j < DEPTH; j++) begin
          intensity_buf[bank][i][j] = sim_helper_random.range(8'hFF, 0);
          phase_buf[bank][i][j] = sim_helper_random.range(8'hFF, 0);
        end
      end
      sim_helper_bram.write_emission_raw_intensity_phase(bank, intensity_buf[bank],
                                                     phase_buf[bank], cycle_buf[bank]);
      for (int i = 0; i < DEPTH; i++) begin
        output_mask_buf[bank][i] = sim_helper_random.range(1'b1, 1'b0);
      end
      sim_helper_bram.write_output_mask(bank, output_mask_buf[bank]);
    end

    $display("memory initialized");

    fork
      update(0, 32'hFFFFFFFF);
      wait_bank(0);
    join
    check(0);

    fork
      update(1, 32'd0);
      wait_bank(1);
    join
    check(1);

    $display("OK! sim_output_mask");
    $finish();
  end

endmodule
