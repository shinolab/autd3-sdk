`timescale 1ns / 1ps
`default_nettype none
module emission_raw #(
    parameter int DEPTH = 249
) (
    input wire CLK,
    input wire START,
    input wire [15:0] IDX,
    emission_bus_if.out_raw_port EMISSION_BUS,
    output wire [7:0] INTENSITY,
    output wire [7:0] PHASE,
    output wire DOUT_VALID
);

  logic [7:0] intensity;
  logic [7:0] phase;
  logic dout_valid;

  logic [63:0] data_out;

  logic [7:0] addr;
  logic [$clog2(DEPTH)-1:0] cnt;

  typedef enum logic [1:0] {
    WAITING,
    BRAM_WAIT_0,
    BRAM_WAIT_1,
    RUN
  } state_t;

  (* fsm_encoding = "none" *) state_t state = WAITING;

  assign EMISSION_BUS.RAW_IDX = IDX[9:0];
  assign EMISSION_BUS.RAW_ADDR = addr;
  assign EMISSION_BUS.RAW_RD_EN = (state != WAITING);
  assign data_out = EMISSION_BUS.VALUE;

  assign INTENSITY = intensity;
  assign PHASE = phase;
  assign DOUT_VALID = dout_valid;

  always_ff @(posedge CLK) begin
    case (state)
      WAITING: begin
        dout_valid <= 1'b0;
        if (START) begin
          addr  <= '0;
          state <= BRAM_WAIT_0;
        end
      end
      BRAM_WAIT_0: begin
        addr  <= addr + 1;
        state <= BRAM_WAIT_1;
      end
      BRAM_WAIT_1: begin
        cnt   <= '0;
        addr  <= addr + 1;
        state <= RUN;
      end
      RUN: begin
        addr <= addr + 1;
        dout_valid <= 1;
        cnt <= cnt + 1;
        {intensity, phase} <= data_out[16*cnt[1:0]+:16];
        state <= (cnt == DEPTH - 1) ? WAITING : state;
      end
      default: state <= WAITING;
    endcase
  end

endmodule
`default_nettype wire
