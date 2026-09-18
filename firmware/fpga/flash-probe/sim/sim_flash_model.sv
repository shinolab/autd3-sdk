`timescale 1ns / 1ps
module sim_flash_model #(
    parameter int ERASE_NS   = 3000000,
    parameter int PROGRAM_NS = 800000
) (
    input wire SCK,
    input wire CS_N,
    input wire MOSI,
    output var MISO
);

  localparam logic [23:0] JEDEC_ID = 24'h20BA18;
  localparam int unsigned WRITABLE_BASE = 32'h800000;

  typedef enum {
    OPCODE,
    ADDRESS,
    RESP_ID,
    RESP_SR,
    RESP_FSR,
    RESP_READ,
    PROGRAM_DATA,
    COMPLETE
  } phase_t;

  logic [7:0] mem[int unsigned];
  int violations = 0;
  int selects = 0;
  int erases = 0;
  int programs = 0;
  bit stuck_wip = 1'b0;

  logic wel = 1'b0;
  logic wip = 1'b0;

  phase_t phase = OPCODE;
  logic [7:0] opcode = 8'h00;
  int in_bits = 0;
  logic [31:0] in_shift = 32'd0;
  int unsigned addr = 0;
  int unsigned page = 0;
  int data_bits = 0;
  logic [7:0] data_byte = 8'h00;
  logic [7:0] out_byte = 8'h00;
  int out_bit = 0;
  int id_idx = 0;

  initial MISO = 1'b0;

  function automatic logic [7:0] peek(input int unsigned a);
    return mem.exists(a) ? mem[a] : 8'hFF;
  endfunction

  function automatic void violation(input string msg);
    $display("VIOLATION: %s at %t", msg, $time);
    violations++;
  endfunction

  task automatic hold_wip(input int ns);
    wip = 1'b1;
    fork
      begin
        #(ns);
        if (!stuck_wip) begin
          wip = 1'b0;
        end
      end
    join_none
  endtask

  always @(negedge CS_N) begin
    selects++;
    phase = OPCODE;
    in_bits = 0;
    data_bits = 0;
    out_bit = 0;
    id_idx = 0;
  end

  always @(posedge CS_N) begin
    if (phase == COMPLETE && opcode == 8'h06) begin
      wel = 1'b1;
    end else if (phase == COMPLETE && opcode == 8'h20) begin
      if (!wel) begin
        violation("sector erase without WREN");
      end else if (addr < WRITABLE_BASE) begin
        violation($sformatf("sector erase of protected address %06h", addr));
      end else begin
        for (int unsigned i = addr & ~32'hFFF; i < (addr & ~32'hFFF) + 32'h1000; i++) begin
          mem.delete(i);
        end
        erases++;
      end
      wel = 1'b0;
      hold_wip(ERASE_NS);
    end else if (phase == PROGRAM_DATA) begin
      if (data_bits == 0) begin
        violation("page program without data");
      end
      if (data_bits % 8 != 0) begin
        violation("page program ended mid-byte");
      end
      programs++;
      wel = 1'b0;
      hold_wip(PROGRAM_NS);
    end
    phase = OPCODE;
  end

  always @(MOSI) begin
    if (!CS_N && SCK) begin
      violation("MOSI changed while SCK is high");
    end
  end

  always @(posedge SCK) begin
    if (!CS_N) begin
      case (phase)
        OPCODE: begin
          in_shift = {in_shift[30:0], MOSI};
          in_bits++;
          if (in_bits == 8) begin
            opcode = in_shift[7:0];
            if (wip && opcode != 8'h05 && opcode != 8'h70) begin
              violation($sformatf("opcode %02h while WIP", opcode));
            end
            case (opcode)
              8'h9F: phase = RESP_ID;
              8'h05: phase = RESP_SR;
              8'h70: phase = RESP_FSR;
              8'h03, 8'h20, 8'h02: phase = ADDRESS;
              8'h06: phase = COMPLETE;
              default: begin
                violation($sformatf("opcode %02h is not allowed", opcode));
                phase = COMPLETE;
              end
            endcase
          end
        end
        ADDRESS: begin
          in_shift = {in_shift[30:0], MOSI};
          in_bits++;
          if (in_bits == 32) begin
            addr = in_shift[23:0];
            page = addr >> 8;
            case (opcode)
              8'h03: phase = RESP_READ;
              8'h20: phase = COMPLETE;
              default: begin
                if (!wel) begin
                  violation("page program without WREN");
                end
                if (addr < WRITABLE_BASE) begin
                  violation($sformatf("page program of protected address %06h", addr));
                end
                phase = PROGRAM_DATA;
              end
            endcase
          end
        end
        PROGRAM_DATA: begin
          data_byte = {data_byte[6:0], MOSI};
          data_bits++;
          if (data_bits % 8 == 0) begin
            if ((addr >> 8) != page) begin
              violation("page program crossed a page boundary");
            end
            if (wel && addr >= WRITABLE_BASE) begin
              mem[addr] = peek(addr) & data_byte;
            end
            addr++;
          end
        end
        COMPLETE: begin
          violation("extra clocks after a complete command");
        end
        default: begin
        end
      endcase
    end
  end

  always @(negedge SCK) begin
    if (!CS_N && (phase == RESP_ID || phase == RESP_SR || phase == RESP_FSR || phase == RESP_READ)) begin
      if (out_bit == 0) begin
        case (phase)
          RESP_ID: begin
            out_byte = JEDEC_ID[23-8*id_idx-:8];
            id_idx   = (id_idx + 1) % 3;
          end
          RESP_SR:  out_byte = {6'd0, wel, wip};
          RESP_FSR: out_byte = {~wip, 7'd0};
          default: begin
            out_byte = peek(addr);
            addr++;
          end
        endcase
        out_bit = 8;
      end
      out_bit--;
      MISO <= #3 out_byte[out_bit];
    end
  end

endmodule
