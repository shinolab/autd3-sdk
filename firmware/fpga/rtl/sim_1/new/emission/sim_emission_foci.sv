`timescale 1ns / 1ps
module sim_emission_foci ();

  `include "define.vh"

  logic CLK;
  logic locked;
  logic [56:0] SYS_TIME;

  localparam int DEPTH = 249;
  localparam int SIZE = 16;
  localparam int NumFoci = 8;

  sim_helper_bram sim_helper_bram ();
  sim_helper_random sim_helper_random ();

  settings::pattern_settings_t pattern_settings;

  logic [13:0] cycle_buf[params::NumBanks];
  logic [31:0] freq_div_buf[params::NumBanks];
  logic signed [17:0] focus_x[params::NumBanks][SIZE*NumFoci];
  logic signed [17:0] focus_y[params::NumBanks][SIZE*NumFoci];
  logic signed [17:0] focus_z[params::NumBanks][SIZE*NumFoci];
  logic [7:0] intensity_and_offsets_buf[params::NumBanks][SIZE*NumFoci];

  logic [15:0] debug_idx;
  logic debug_bank;
  logic [7:0] intensity;
  logic [7:0] phase;
  logic dout_valid;

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
      .DEPTH(DEPTH),
      .MODE ("TRUNC")
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

  task automatic update(input logic req_bank, input logic [31:0] rep);
    @(posedge CLK);
    pattern_settings.UPDATE <= 1'b1;
    pattern_settings.REQ_RD_BANK <= req_bank;
    pattern_settings.REP[req_bank] <= rep;
    pattern_settings.CYCLE[req_bank] = cycle_buf[req_bank] - 1;
    pattern_settings.FREQ_DIV[req_bank] = freq_div_buf[req_bank];
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

  logic [7:0] sin_table [  256];
  logic [7:0] atan_table[16384];

  task automatic check(input logic bank);
    automatic int idx, ix, iy;
    automatic int debug_idx_buf;
    automatic logic signed [63:0] x, y, z;
    automatic logic [63:0] r, lambda;
    automatic logic [7:0] p, phase_expect;
    automatic logic [10:0] cos, sin;
    automatic logic [7:0] cos_buf[NumFoci], sin_buf[NumFoci];

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
      $display("check %d @%d", debug_idx, SYS_TIME);
      idx = 0;
      debug_idx_buf = debug_idx;
      for (int id = 0; idx < DEPTH; id++) begin
        ix = id % 18;
        iy = id / 18;
        if ((iy === 1) && (ix === 1 || ix === 2 || ix === 16)) begin
          continue;
        end
        for (int k = 0; k < NumFoci; k++) begin
          x = focus_x[bank][debug_idx_buf*NumFoci+k] - int'(10.16 * ix / 0.025);  // [0.025mm]
          y = focus_y[bank][debug_idx_buf*NumFoci+k] - int'(10.16 * iy / 0.025);  // [0.025mm]
          z = focus_z[bank][debug_idx_buf*NumFoci+k];  // [0.025mm]
          r = $rtoi($sqrt($itor(x * x + y * y + z * z)));  // [0.025mm]
          lambda = (r << 14) / pattern_settings.SOUND_SPEED[bank];
          p = lambda % 256;
          if (k !== 0) begin
            p += intensity_and_offsets_buf[bank][debug_idx_buf*NumFoci+k];
          end
          sin_buf[k] = sin_table[p%256];
          cos_buf[k] = sin_table[(p+64)%256];
        end
        cos = 0;
        sin = 0;
        for (int k = 0; k < NumFoci; k++) begin
          cos += cos_buf[k];
          sin += sin_buf[k];
        end
        sin /= NumFoci;
        cos /= NumFoci;
        phase_expect = atan_table[{sin[7:1], cos[7:1]}];
        `ASSERT_EQ(intensity_and_offsets_buf[bank][debug_idx_buf*NumFoci], intensity);
        `ASSERT_EQ(phase_expect, phase);
        @(posedge CLK);
        idx++;
      end
    end
  endtask

  initial begin
    $readmemh("sin.txt", sin_table);
    $readmemh("atan.txt", atan_table);

    sim_helper_random.init();

    cycle_buf[0] = SIZE;
    cycle_buf[1] = SIZE / 4;
    freq_div_buf[0] = 1;
    freq_div_buf[1] = 3;

    pattern_settings.UPDATE = 0;
    pattern_settings.TRANSITION_MODE = params::TRANSITION_MODE_SYNC_IDX;
    pattern_settings.TRANSITION_VALUE = 0;
    pattern_settings.MODE[0] = params::EMISSION_TYPE_FOCI;
    pattern_settings.MODE[1] = params::EMISSION_TYPE_FOCI;
    pattern_settings.SOUND_SPEED[0] = 340 * 64;
    pattern_settings.SOUND_SPEED[1] = 340 * 64;
    pattern_settings.CYCLE[0] = '0;
    pattern_settings.FREQ_DIV[0] = '1;
    pattern_settings.NUM_FOCI[0] = NumFoci;
    pattern_settings.CYCLE[1] = '0;
    pattern_settings.FREQ_DIV[1] = '1;
    pattern_settings.NUM_FOCI[1] = NumFoci;

    @(posedge locked);

    for (int bank = 0; bank < params::NumBanks; bank++) begin
      for (int i = 0; i < SIZE; i++) begin
        for (int k = 0; k < NumFoci; k++) begin
          focus_x[bank][i*NumFoci+k] = sim_helper_random.range(131071, -131072 + 6908);
          focus_y[bank][i*NumFoci+k] = sim_helper_random.range(131071, -131072 + 5283);
          focus_z[bank][i*NumFoci+k] = sim_helper_random.range(131071, -131072);
          intensity_and_offsets_buf[bank][i*NumFoci+k] = sim_helper_random.range(8'hFF, 0);
        end
      end
      sim_helper_bram.write_emission_focus(bank, focus_x[bank], focus_y[bank], focus_z[bank],
                                      intensity_and_offsets_buf[bank],
                                      cycle_buf[bank] * NumFoci);
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

    $display("OK! sim_emission_foci");
    $finish();
  end

endmodule
