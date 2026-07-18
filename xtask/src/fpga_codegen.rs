use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

use crate::util::{on_path, run};

const BRAM_DATA_WIDTH: u32 = 16;

#[derive(Clone)]
enum Def {
    Int(i64),
    Str(&'static str),
}

#[derive(Clone)]
struct Param {
    addr: String,
    width: u32,
    name: String,
    default: Def,
}

impl Param {
    fn new(addr: &str, width: u32, name: &str, default: Def) -> Self {
        Self {
            addr: addr.to_string(),
            width,
            name: name.to_string(),
            default,
        }
    }

    fn null() -> Self {
        Self {
            addr: String::new(),
            width: 0,
            name: String::new(),
            default: Def::Int(0),
        }
    }
}

struct Group {
    name: &'static str,
    params: Vec<Param>,
}

struct State {
    name: String,
    req: Param,
    param: Param,
}

fn param_groups() -> Vec<Group> {
    vec![
        mod_group(),
        pattern_group(),
        silencer_group(),
        debug_group(),
        sync_group(),
    ]
}

fn mod_group() -> Group {
    use Def::{Int, Str};
    Group {
        name: "MOD",
        params: vec![
            Param::new("MOD_REQ_RD_BANK", 1, "REQ_RD_BANK", Int(0)),
            Param::new(
                "MOD_TRANSITION_MODE",
                8,
                "TRANSITION_MODE",
                Str("params::TRANSITION_MODE_SYNC_IDX"),
            ),
            Param::new("MOD_TRANSITION_VALUE", 64, "TRANSITION_VALUE", Int(0)),
            Param::new("MOD_CYCLE0", 16, "CYCLE[0]", Int(1)),
            Param::new("MOD_CYCLE1", 16, "CYCLE[1]", Int(1)),
            Param::new("MOD_FREQ_DIV0", 16, "FREQ_DIV[0]", Int(10)),
            Param::new("MOD_FREQ_DIV1", 16, "FREQ_DIV[1]", Int(10)),
            Param::new("MOD_REP0", 16, "REP[0]", Str("16'hFFFF")),
            Param::new("MOD_REP1", 16, "REP[1]", Str("16'hFFFF")),
        ],
    }
}

fn pattern_group() -> Group {
    use Def::{Int, Str};
    Group {
        name: "PATTERN",
        params: vec![
            Param::new("PATTERN_REQ_RD_BANK", 1, "REQ_RD_BANK", Int(0)),
            Param::new(
                "PATTERN_TRANSITION_MODE",
                8,
                "TRANSITION_MODE",
                Str("params::TRANSITION_MODE_SYNC_IDX"),
            ),
            Param::new("PATTERN_TRANSITION_VALUE", 64, "TRANSITION_VALUE", Int(0)),
            Param::new(
                "PATTERN_MODE0",
                1,
                "MODE[0]",
                Str("params::EMISSION_TYPE_RAW"),
            ),
            Param::new(
                "PATTERN_MODE1",
                1,
                "MODE[1]",
                Str("params::EMISSION_TYPE_RAW"),
            ),
            Param::new("PATTERN_CYCLE0", 16, "CYCLE[0]", Int(0)),
            Param::new("PATTERN_CYCLE1", 16, "CYCLE[1]", Int(0)),
            Param::new("PATTERN_FREQ_DIV0", 16, "FREQ_DIV[0]", Str("16'hFFFF")),
            Param::new("PATTERN_FREQ_DIV1", 16, "FREQ_DIV[1]", Str("16'hFFFF")),
            Param::new("PATTERN_SOUND_SPEED0", 16, "SOUND_SPEED[0]", Int(0)),
            Param::new("PATTERN_SOUND_SPEED1", 16, "SOUND_SPEED[1]", Int(0)),
            Param::new("PATTERN_REP0", 16, "REP[0]", Str("16'hFFFF")),
            Param::new("PATTERN_REP1", 16, "REP[1]", Str("16'hFFFF")),
            Param::new("PATTERN_NUM_FOCI0", 8, "NUM_FOCI[0]", Str("1")),
            Param::new("PATTERN_NUM_FOCI1", 8, "NUM_FOCI[1]", Str("1")),
        ],
    }
}

fn silencer_group() -> Group {
    use Def::Int;
    Group {
        name: "SILENCER",
        params: vec![
            Param::new("SILENCER_FLAG", 8, "FLAG", Int(0)),
            Param::new(
                "SILENCER_UPDATE_RATE_INTENSITY",
                16,
                "UPDATE_RATE_INTENSITY",
                Int(256),
            ),
            Param::new(
                "SILENCER_UPDATE_RATE_PHASE",
                16,
                "UPDATE_RATE_PHASE",
                Int(256),
            ),
            Param::new(
                "SILENCER_COMPLETION_STEPS_INTENSITY",
                16,
                "COMPLETION_STEPS_INTENSITY",
                Int(10),
            ),
            Param::new(
                "SILENCER_COMPLETION_STEPS_PHASE",
                16,
                "COMPLETION_STEPS_PHASE",
                Int(40),
            ),
        ],
    }
}

fn debug_group() -> Group {
    use Def::Str;
    let debug_default = "{params::GPIO_O_TYPE_NONE, 56'd0}";
    Group {
        name: "DEBUG",
        params: vec![
            Param::new("DEBUG_VALUE0", 64, "VALUE[0]", Str(debug_default)),
            Param::new("DEBUG_VALUE1", 64, "VALUE[1]", Str(debug_default)),
            Param::new("DEBUG_VALUE2", 64, "VALUE[2]", Str(debug_default)),
            Param::new("DEBUG_VALUE3", 64, "VALUE[3]", Str(debug_default)),
        ],
    }
}

fn sync_group() -> Group {
    use Def::Int;
    Group {
        name: "SYNC",
        params: vec![
            Param::new("ECAT_SYNC_TIME", 64, "ECAT_SYNC_TIME", Int(0)),
            Param::new("ECAT_SYNC_CYCLE", 32, "ECAT_SYNC_CYCLE", Int(0)),
        ],
    }
}

fn split_params(params: &[Param]) -> Vec<Param> {
    let mut out = Vec::new();
    for param in params {
        if param.width <= BRAM_DATA_WIDTH {
            out.push(param.clone());
        } else {
            let n = param.width / BRAM_DATA_WIDTH;
            for i in 0..n {
                let hi = param.width.min(BRAM_DATA_WIDTH * (i + 1)) - 1;
                let lo = BRAM_DATA_WIDTH * i;
                out.push(Param {
                    addr: format!("{}_{i}", param.addr),
                    width: BRAM_DATA_WIDTH.min(param.width - BRAM_DATA_WIDTH * i),
                    name: format!("{}[{hi}:{lo}]", param.name),
                    default: param.default.clone(),
                });
            }
        }
    }
    out
}

fn gen_state(req: Param, param: Param) -> State {
    let mut name = if req.addr.is_empty() {
        String::new()
    } else {
        format!("REQ_{}", req.addr)
    };
    if !param.addr.is_empty() {
        name = if name.is_empty() {
            format!("RD_{}", param.addr)
        } else {
            format!("{name}_RD_{}", param.addr)
        };
    }
    State { name, req, param }
}

fn gen_states(group: &Group) -> Vec<State> {
    let sub = split_params(&group.params);
    let n = sub.len();
    let mut states = Vec::new();
    for k in 0..n + 3 {
        let req = if k < n { sub[k].clone() } else { Param::null() };
        let param = if k < 3 {
            Param::null()
        } else {
            sub[k - 3].clone()
        };
        states.push(gen_state(req, param));
    }
    states.push(State {
        name: format!("{}_CLR_UPDATE_SETTINGS_BIT", group.name),
        req: Param::null(),
        param: Param::null(),
    });
    states
}

fn dout_range(width: u32) -> String {
    if width == BRAM_DATA_WIDTH {
        String::new()
    } else if width == 1 {
        "[0]".to_string()
    } else {
        format!("[{}:0]", width - 1)
    }
}

fn generate(groups: &[Group]) -> String {
    let all_states: Vec<(&str, Vec<State>)> =
        groups.iter().map(|g| (g.name, gen_states(g))).collect();

    let mut out = String::new();
    push_header(&mut out, &all_states);
    push_dispatch(&mut out, &all_states);
    push_bodies(&mut out, &all_states);
    push_initial(&mut out, groups);
    out
}

fn push_header(out: &mut String, all_states: &[(&str, Vec<State>)]) {
    let total: usize = all_states.iter().map(|(_, s)| s.len()).sum();
    let enum_width = (u32::try_from(8 + total).unwrap() - 1).ilog2() + 1;

    out.push_str(
        "`timescale 1ns / 1ps\n`default_nettype none\nmodule controller (\n    input wire CLK,\n    input wire ENABLE,\n    input wire THERMO,\n    input wire PATTERN_BANK,\n    input wire MOD_BANK,\n    input wire [15:0] PATTERN_CYCLE,\n    input wire PATTERN_STOPPED,\n    input wire MOD_STOPPED,\n    input wire TRANSITION_PENDING,\n    input wire [7:0] SYNC_RESYNC_COUNT,\n    cnt_bus_if.out_port cnt_bus,\n    output var settings::mod_settings_t MOD_SETTINGS,\n    output var settings::pattern_settings_t PATTERN_SETTINGS,\n    output var settings::silencer_settings_t SILENCER_SETTINGS,\n    output var settings::sync_settings_t SYNC_SETTINGS,\n    output var settings::debug_settings_t DEBUG_SETTINGS,\n    output var FORCE_FAN,\n    output var GPIO_IN[4]\n);\n\n  localparam bit [7:0] FunctionBits = (1'b0 << params::FuncDynamicFreqBit)\n                                      | (1'b0 << params::FuncEmulatorBit);\n\n  logic [15:0] ctl_flags = '0;\n\n  logic we = 1'b0;\n  logic [7:0]  addr;\n  logic [15:0] din;\n  logic [15:0] dout;\n\n  logic [15:0] fpga_state_prev = 16'hFFFF;\n\n  assign cnt_bus.WE = we;\n  assign cnt_bus.ADDR = addr;\n  assign cnt_bus.DIN = din;\n  assign dout = cnt_bus.DOUT;\n\n  assign FORCE_FAN = ctl_flags[params::CTL_FLAG_BIT_FORCE_FAN];\n  assign GPIO_IN[0] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_0];\n  assign GPIO_IN[1] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_1];\n  assign GPIO_IN[2] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_2];\n  assign GPIO_IN[3] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_3];\n\n  function automatic logic [15:0] fpga_state_din();\n    return {\n      SYNC_RESYNC_COUNT,\n      1'h0  /* reserved */,\n      TRANSITION_PENDING,\n      MOD_STOPPED,\n      PATTERN_STOPPED,\n      PATTERN_CYCLE == '0,\n      PATTERN_BANK,\n      MOD_BANK,\n      THERMO\n    };\n  endfunction\n\n",
    );
    writeln!(out, "  typedef enum logic [{}:0] {{", enum_width - 1).unwrap();
    out.push_str(
        "    REQ_WR_VER_PATCH,\n    REQ_WR_VER_MINOR,\n    REQ_WR_VER,\n    WAIT_WR_VER_0_REQ_RD_CTL_FLAG,\n    WR_VER_MINOR_WAIT_RD_CTL_FLAG_BIT_0,\n    WR_VER_WAIT_RD_CTL_FLAG_BIT_1,\n    WAIT_0,\n    WAIT_1,\n",
    );
    let enum_body: Vec<String> = all_states
        .iter()
        .flat_map(|(_, states)| states.iter())
        .map(|s| format!("    {}", s.name))
        .collect();
    out.push_str(&enum_body.join(",\n"));
    out.push_str("\n  } state_t;\n\n  state_t state = REQ_WR_VER_PATCH;\n");
}

fn push_dispatch(out: &mut String, all_states: &[(&str, Vec<State>)]) {
    out.push_str(
        "\n  always_ff @(posedge CLK) begin\n    if (!ENABLE) begin\n      state <= REQ_WR_VER_PATCH;\n      we <= 1'b0;\n      fpga_state_prev <= 16'hFFFF;\n    end else case (state)\n      REQ_WR_VER_PATCH: begin\n        we <= 1'b1;\n\n        din <= {8'd0, params::VersionNumPatch};\n        addr <= params::ADDR_VERSION_NUM_PATCH;\n\n        state <= REQ_WR_VER_MINOR;\n      end\n      REQ_WR_VER_MINOR: begin\n        din <= {8'd0, params::VersionNumMinor};\n        addr <= params::ADDR_VERSION_NUM_MINOR;\n\n        state <= REQ_WR_VER;\n      end\n      REQ_WR_VER: begin\n        din   <= {FunctionBits, params::VersionNumMajor};\n        addr  <= params::ADDR_VERSION_NUM_MAJOR;\n\n        state <= WAIT_WR_VER_0_REQ_RD_CTL_FLAG;\n      end\n      WAIT_WR_VER_0_REQ_RD_CTL_FLAG: begin\n        we <= 1'b0;\n        addr <= params::ADDR_CTL_FLAG;\n\n        state <= WR_VER_MINOR_WAIT_RD_CTL_FLAG_BIT_0;\n      end\n      WR_VER_MINOR_WAIT_RD_CTL_FLAG_BIT_0: begin\n        state <= WR_VER_WAIT_RD_CTL_FLAG_BIT_1;\n      end\n      WR_VER_WAIT_RD_CTL_FLAG_BIT_1: begin\n        state <= WAIT_0;\n      end\n\n      WAIT_0: begin\n        addr <= params::ADDR_FPGA_STATE;\n        if (fpga_state_din() != fpga_state_prev) begin\n          we <= 1'b1;\n          din <= fpga_state_din();\n          fpga_state_prev <= fpga_state_din();\n        end else begin\n          we <= 1'b0;\n        end\n\n       ",
    );

    for (name, states) in all_states {
        write!(
            out,
            " if (ctl_flags[params::CTL_FLAG_BIT_{name}_SET]) begin\n          ctl_flags <= ctl_flags & ~(1 << params::CTL_FLAG_BIT_{name}_SET);\n          state <= {};\n        end else",
            states[0].name
        )
        .unwrap();
    }
    out.push_str(
        " begin\n          ctl_flags <= dout;\n          state <= WAIT_1;\n        end\n      end\n      WAIT_1: begin\n        we <= 1'b0;\n        addr <= params::ADDR_CTL_FLAG;\n        state <= WAIT_0;\n      end\n",
    );
}

fn push_bodies(out: &mut String, all_states: &[(&str, Vec<State>)]) {
    for (name, states) in all_states {
        let len = states.len();
        for (i, state) in states.iter().enumerate() {
            write!(out, "\n      {}: begin", state.name).unwrap();

            if i == 0 {
                out.push_str("\n        we <= 1'b0;");
            }

            if !state.req.addr.is_empty() {
                write!(out, "\n        addr <= params::ADDR_{};", state.req.addr).unwrap();
            }

            if !state.param.addr.is_empty() {
                write!(
                    out,
                    "\n        {name}_SETTINGS.{} <= dout{};",
                    state.param.name,
                    dout_range(state.param.width)
                )
                .unwrap();
            }

            if i == len - 4 {
                out.push_str(
                    "\n        we <= 1'b1;\n        addr <= params::ADDR_CTL_FLAG;\n        din <= ctl_flags;",
                );
            }

            if i == len - 3 {
                out.push_str(
                    "\n        we <= 1'b1;\n        addr <= params::ADDR_FPGA_STATE;\n        din  <= fpga_state_din();\n        fpga_state_prev <= fpga_state_din();",
                );
            }

            if i == len - 2 {
                write!(
                    out,
                    "\n        {name}_SETTINGS.UPDATE <= 1'b1;\n        we <= 1'b0;\n        addr <= params::ADDR_CTL_FLAG;"
                )
                .unwrap();
            }

            if i + 1 < len {
                write!(out, "\n        state <= {};", states[i + 1].name).unwrap();
            }

            if i == len - 1 {
                write!(
                    out,
                    "\n        we <= 1'b1;\n        addr <= params::ADDR_FPGA_STATE;\n        din  <= fpga_state_din();\n        fpga_state_prev <= fpga_state_din();\n        ctl_flags <= dout;\n        {name}_SETTINGS.UPDATE <= 1'b0;\n        state <= WAIT_1;"
                )
                .unwrap();
            }

            out.push_str("\n      end");
        }
        out.push('\n');
    }

    out.push_str("\n      default: state <= WAIT_0;\n    endcase\n  end\n");
}

fn push_initial(out: &mut String, groups: &[Group]) {
    out.push_str("\n  initial begin");
    for group in groups {
        write!(out, "\n    {}_SETTINGS.UPDATE = 1'b0;", group.name).unwrap();
        for param in &group.params {
            if param.name.is_empty() {
                continue;
            }
            let default_value = match &param.default {
                Def::Str(s) => (*s).to_string(),
                Def::Int(v) => format!("{}'d{v}", param.width),
            };
            write!(
                out,
                "\n    {}_SETTINGS.{} = {default_value};",
                group.name, param.name
            )
            .unwrap();
        }
    }
    out.push_str("\n  end\n");

    out.push_str("\nendmodule\n`default_nettype wire\n");
}

pub fn gen_controller(fpga_dir: &Path) -> Result<()> {
    let path = fpga_dir.join("rtl/sources_1/new/controller/controller.sv");
    let contents = generate(&param_groups());
    std::fs::write(&path, contents)
        .with_context(|| format!("failed to write {}", path.display()))?;

    if !on_path("verible-verilog-format") {
        anyhow::bail!(
            "verible-verilog-format not found on PATH. Install Verible so the generated \
             controller.sv can be formatted."
        );
    }
    run(
        "verible-verilog-format",
        [
            path.to_string_lossy().as_ref(),
            crate::fpga::VERIBLE_COLUMN_LIMIT,
            "--inplace",
        ],
        fpga_dir,
    )?;

    println!("generated {}", path.display());
    Ok(())
}
