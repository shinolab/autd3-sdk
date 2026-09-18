`timescale 1ns / 1ps
`default_nettype none
module memory (
    input wire CLK,
    memory_bus_if.bram_port MEM_BUS,
    cnt_bus_if.in_port CNT_BUS,
    phase_corr_bus_if.in_port PHASE_CORR_BUS,
    output_mask_bus_if.in_port OUTPUT_MASK_BUS,
    modulation_bus_if.in_port MOD_BUS,
    emission_bus_if.in_port EMISSION_BUS,
    pwe_table_bus_if.in_port PWE_TABLE_BUS,
    flash_bus_if.host_port FLASH_BUS
);

  import params::*;

  logic bus_clk;
  logic en;
  logic we;
  logic [1:0] select;
  logic [13:0] addr;
  logic [15:0] data_in;
  logic [15:0] data_out;
  logic [15:0] ctl_dout;
  logic [15:0] flash_dout;
  logic flash_rd_sel = 1'b0;

  assign bus_clk = MEM_BUS.BUS_CLK;
  assign select = MEM_BUS.BRAM_SELECT;
  assign en = MEM_BUS.EN;
  assign we = MEM_BUS.WE;
  assign addr = MEM_BUS.BRAM_ADDR;
  assign data_in = MEM_BUS.DATA_IN;
  assign MEM_BUS.DATA_OUT = data_out;
  assign data_out = flash_rd_sel ? flash_dout : ctl_dout;

  logic [5:0] cnt_sel;
  assign cnt_sel = addr[13:8];

  ///////////////////////////// Controller ////////////////////////////
  logic ctl_en;

  assign ctl_en = (cnt_sel == BRAM_CNT_SELECT_MAIN) & (select == BRAM_SELECT_CONTROLLER) & en;

  BRAM_CONTROLLER ctl_bram (
      .clka (bus_clk),
      .ena  (ctl_en),
      .wea  (we),
      .addra(addr[7:0]),
      .dina (data_in),
      .douta(ctl_dout),
      .clkb (CLK),
      .web  (CNT_BUS.WE),
      .addrb(CNT_BUS.ADDR),
      .dinb (CNT_BUS.DIN),
      .doutb(CNT_BUS.DOUT)
  );
  ///////////////////////////// Controller ////////////////////////////

  localparam logic [5:0] FlashBufSelect = BRAM_CNT_SELECT_FLASH_BUF;

  logic flash_reg_en;
  logic flash_buf_en;

  assign flash_reg_en = (cnt_sel == BRAM_CNT_SELECT_FLASH) & (select == BRAM_SELECT_CONTROLLER) & en;
  assign flash_buf_en = (cnt_sel[5:1] == FlashBufSelect[5:1]) & (select == BRAM_SELECT_CONTROLLER) & en;

  always_ff @(posedge bus_clk) begin
    if (flash_reg_en) begin
      flash_rd_sel <= 1'b1;
    end else if (ctl_en) begin
      flash_rd_sel <= 1'b0;
    end
  end

  flash_host flash_host (
      .BUS_CLK(bus_clk),
      .CLK(CLK),
      .REG_EN(flash_reg_en),
      .BUF_EN(flash_buf_en),
      .WE(we),
      .ADDR(addr[8:0]),
      .DATA_IN(data_in),
      .DATA_OUT(flash_dout),
      .FLASH_BUS(FLASH_BUS)
  );

  ///////////////////////// Phase correction //////////////////////////
  logic phase_corr_en;

  logic [7:0] phase_corr_idx;
  logic [7:0] phase_corr_dout;

  assign phase_corr_en = (cnt_sel == BRAM_CNT_SELECT_PHASE_CORR) & (select == BRAM_SELECT_CONTROLLER) & en;
  assign phase_corr_idx = PHASE_CORR_BUS.IDX;
  assign PHASE_CORR_BUS.VALUE = phase_corr_dout;

  BRAM_PHASE_CORR phase_corr_bram (
      .clka (bus_clk),
      .ena  (phase_corr_en),
      .wea  (we),
      .addra(addr[6:0]),
      .dina (data_in),
      .douta(),
      .clkb (CLK),
      .enb  (PHASE_CORR_BUS.RD_EN),
      .web  (1'b0),
      .addrb(phase_corr_idx),
      .dinb (),
      .doutb(phase_corr_dout)
  );
  ///////////////////////// Phase correction //////////////////////////

  //////////////////////////// Output mask ////////////////////////////
  logic output_mask_en;

  logic [255:0] output_mask_dout;

  assign output_mask_en = (cnt_sel == BRAM_CNT_SELECT_OUTPUT_MASK) & (select == BRAM_SELECT_CONTROLLER) & en;
  assign OUTPUT_MASK_BUS.VALUE = output_mask_dout;
  BRAM_OUTPUT_MASK output_mask_bram (
      .clka (bus_clk),
      .ena  (output_mask_en),
      .wea  (we),
      .addra(addr[4:0]),
      .dina (data_in),
      .douta(),
      .clkb (CLK),
      .enb  (OUTPUT_MASK_BUS.RD_EN),
      .web  (1'b0),
      .addrb(1'b0),
      .dinb (),
      .doutb(output_mask_dout)
  );
  //////////////////////////// Output mask ////////////////////////////

  ///////////////////////////// PWE table ////////////////////////////
  logic pwe_table_en;

  logic [7:0] pwe_table_idx;
  logic [15:0] pwe_table_dout;

  assign pwe_table_en = (select == BRAM_SELECT_PWE_TABLE) & en;
  assign pwe_table_idx = PWE_TABLE_BUS.IDX;
  assign PWE_TABLE_BUS.VALUE = pwe_table_dout[8:0];

  BRAM_PWE_TABLE pwe_table_bram (
      .clka (bus_clk),
      .ena  (pwe_table_en),
      .wea  (we),
      .addra(addr[7:0]),
      .dina (data_in),
      .douta(),
      .clkb (CLK),
      .enb  (PWE_TABLE_BUS.RD_EN),
      .web  (1'b0),
      .addrb(pwe_table_idx),
      .dinb (),
      .doutb(pwe_table_dout)
  );
  ///////////////////////////// PWE table ////////////////////////////

  ///////////////////////////// Modulator /////////////////////////////
  logic mod_en[NumBanks];
  logic mod_rd_en[NumBanks];

  logic [15:0] mod_idx;
  logic [7:0] mod_value[NumBanks];

  logic mod_mem_wr_bank = 1'b0;
  logic mod_mem_wr_page = 1'b0;

  assign mod_idx = MOD_BUS.IDX;
  assign MOD_BUS.VALUE = mod_value[MOD_BUS.BANK];
  for (genvar i = 0; i < NumBanks; i++) begin : gen_mod_bram
    assign mod_en[i] = (select == BRAM_SELECT_MOD) & en & (mod_mem_wr_bank == i);
    assign mod_rd_en[i] = MOD_BUS.RD_EN & (MOD_BUS.BANK == i);
    BRAM_MOD mod_bram (
        .clka (bus_clk),
        .ena  (mod_en[i]),
        .wea  (we),
        .addra({mod_mem_wr_page, addr}),
        .dina (data_in),
        .douta(),
        .clkb (CLK),
        .enb  (mod_rd_en[i]),
        .web  ('0),
        .addrb(mod_idx),
        .dinb ('0),
        .doutb(mod_value[i])
    );
  end
  ///////////////////////////// Modulator /////////////////////////////

  /////////////////////////////    EMISSION   /////////////////////////////
  logic emission_en[NumBanks];
  logic emission_rd_en[NumBanks];

  logic [15:0] pattern_idx;
  logic [63:0] emission_value[NumBanks];

  logic pattern_mem_wr_bank = 1'b0;
  logic [3:0] pattern_mem_wr_page = '0;

  assign pattern_idx = EMISSION_BUS.ADDR;
  assign EMISSION_BUS.VALUE = emission_value[EMISSION_BUS.BANK];
  for (genvar i = 0; i < NumBanks; i++) begin : gen_emission_bram
    assign emission_en[i] = (select == BRAM_SELECT_EMISSION) & en & (pattern_mem_wr_bank == i);
    assign emission_rd_en[i] = EMISSION_BUS.RD_EN & (EMISSION_BUS.BANK == i);
    bram_emission emission_bram (
        .clka (bus_clk),
        .ena  (emission_en[i]),
        .wea  (we),
        .addra({pattern_mem_wr_page, addr}),
        .dina (data_in),
        .clkb (CLK),
        .enb  (emission_rd_en[i]),
        .addrb(pattern_idx),
        .doutb(emission_value[i])
    );
  end
  /////////////////////////////    EMISSION   /////////////////////////////

  logic [2:0] ctl_we_edge = 3'b000;
  always_ff @(posedge bus_clk) begin
    ctl_we_edge <= {ctl_we_edge[1:0], we & ctl_en};
    if (ctl_we_edge == 3'b011) begin
      case (addr)
        ADDR_MOD_MEM_WR_BANK: mod_mem_wr_bank <= data_in[0];
        ADDR_MOD_MEM_WR_PAGE: mod_mem_wr_page <= data_in[0];
        ADDR_PATTERN_MEM_WR_BANK: pattern_mem_wr_bank <= data_in[0];
        ADDR_PATTERN_MEM_WR_PAGE: pattern_mem_wr_page <= data_in[3:0];
        default: begin
        end
      endcase
    end
  end

endmodule
`default_nettype wire
