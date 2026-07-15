# truss-fem

A command-line finite element analyzer for **2D / 3D pin-jointed trusses**,
written in Rust. It computes nodal displacements, support reactions, and member
axial forces/stresses using the **direct stiffness method**, and can render an
interactive, self-contained **web viewer** of the structure and its results.
It ships with an **interactive shell** (`run.bat` / `truss-fem` with no
arguments) and a set of realistic sample models under [`input/`](#sample-models-input).

This is a modern rewrite of the legacy MFC "CauFEM" program (2005). The original
C++ engine only parsed the input and drew an empty window — its solver
(`Action`, `Solve`, `GetForce`, matrix inverse) was a set of empty stubs. This
version implements the full analysis pipeline and fixes the bugs found along the
way (see [Improvements](#improvements-over-the-original)).

---

## Build

```powershell
cd cli
cargo build --release
# binary: target/release/truss-fem(.exe)
```

Requires a Rust toolchain (1.70+). The only dependency is `clap` for argument
parsing; the FEM engine, JSON writer, web viewer, and HTTP server are all
implemented with the standard library.

## Quick start

The easiest entry point is the launcher script, which builds the project and
drops you into the **interactive shell**:

```powershell
.\run.bat            # Windows  (./run.sh on Linux/macOS/Git Bash)
```

```text
truss-fem> samples                          # list bundled models
truss-fem> load input/bridge_pratt_2d.txt   # load one
truss-fem> solve                            # run the analysis, print the report
truss-fem> view                             # open the interactive web viewer
truss-fem> save results.json                # save the report (text or JSON)
truss-fem> exit
```

`run.bat demo` runs a one-shot demo (solve + web viewer), and any other
arguments are forwarded to the CLI: `run.bat solve input\roof_howe_2d.txt`.

## Interactive shell

`truss-fem` with no arguments (or `truss-fem shell`) starts a REPL that keeps
the current model in memory:

| Shell command        | Description                                                      |
|----------------------|------------------------------------------------------------------|
| `load <file>`        | Load a truss model from an input file.                           |
| `info [file]`        | Model summary (loads the file first if a path is given).         |
| `solve [file]`       | Run the FEM analysis and print the report.                       |
| `report [text\|json]`| Re-print the last report in the given format.                    |
| `save <file>`        | Save the last report (`.json` extension → JSON, otherwise text). |
| `save-model <file>`  | Save the current model back to the input file format.            |
| `view [out.html]`    | Solve if needed, write the HTML viewer, and open the browser.    |
| `serve [port]`       | Host the viewer at `http://127.0.0.1:<port>` (default 8080).     |
| `samples`            | List bundled sample models under `input/` and `examples/`.       |
| `example <file>`     | Write a ready-to-run example input file.                         |
| `help` / `exit`      | Show help / leave the shell.                                     |

## Commands (non-interactive)

| Command   | Description                                                             |
|-----------|-------------------------------------------------------------------------|
| `shell`   | Start the interactive shell (default when no subcommand is given).      |
| `solve`   | Analyze a model and print/save a report (`--format text|json`).         |
| `view`    | Generate a standalone interactive HTML viewer (`--open` to launch it).  |
| `serve`   | Host the viewer from a local web server and open the browser.           |
| `info`    | Print a model summary (nodes, elements, supports, DOFs) without solving.|
| `example` | Write a ready-to-run example input file.                                |

Run `truss-fem <command> --help` for all options.

### Examples

```powershell
truss-fem solve input/bridge_pratt_2d.txt                # text report to stdout
truss-fem solve input/building_tower_3d.txt -f json -o out.json
truss-fem view  input/bridge_pratt_3d.txt --open         # writes bridge_pratt_3d.html
truss-fem serve input/transmission_tower_3d.txt --port 8080
truss-fem info  examples/space_truss.txt
```

## Sample models (`input/`)

Ready-to-run structural models in consistent **N–mm** units
(steel, `E = 205000 N/mm²`, Y axis is up):

| File                          | Structure                                                       |
|-------------------------------|-----------------------------------------------------------------|
| `building_tower_3d.txt`       | 3D building tower: 5 stories, 4 legs, X-braced, wind + gravity. |
| `bridge_pratt_2d.txt`         | 2D Pratt bridge: 6 panels, 30 m span, statically determinate.   |
| `bridge_pratt_3d.txt`         | 3D through-truss bridge: two Pratt trusses + lateral/sway bracing. |
| `roof_howe_2d.txt`            | 2D Howe gable roof truss: 16 m span, purlin + ceiling loads.    |
| `transmission_tower_3d.txt`   | 3D tapered lattice transmission tower with conductor pull.      |

The two small classic examples remain under `examples/`
(`plane_truss.txt`, `space_truss.txt`).

## Web viewer

The viewer is a single, dependency-free HTML file (no CDN, works offline):

- **3D orbit view** — drag to rotate, scroll to zoom, `Shift`+drag to pan.
- **Undeformed vs. deformed** shape overlay with an adjustable deformation scale.
- **Members colored by axial force** — red = tension, blue = compression,
  thickness scaled by magnitude; hover any member for its force/stress.
- **Supports and loads** drawn as markers and arrows.
- Sortable summary, member, and displacement tables.
- Preset views (Iso / XY / XZ / YZ) and one-click Fit.

## Input format

Backward-compatible with the original `TrustInput.txt`:

```text
<title>                                     # free-form title line
<numElements>,<numNodes>                    # counts
id, fixX, fixY, fixZ, x, y, z, Fx, Fy, Fz   # one line per node
...
id, nodeA, nodeB, E, A                      # one line per element
...
```

- `fixX/fixY/fixZ`: `1` = the DOF is a support (fixed), `0` = free.
- `x, y, z`: node coordinates. `Fx, Fy, Fz`: applied nodal load.
- Elements reference nodes by **id**; `E` is Young's modulus, `A` is the area.
- Blank lines and `# comments` are ignored; fields may be separated by commas
  and/or whitespace.

> For a **planar (2D) truss** in the XY plane, fix the out-of-plane `z` DOF at
> every node (`fixZ = 1`), otherwise the structure is unstable out of plane and
> the solver will (correctly) report a singular/mechanism error.

### Example (`examples/space_truss.txt`)

```text
SPACE TRUSS EXAMPLE OF SECTION
3,4
1,0,1,0,72.0,0.,0.,0.,0.,-1000.0
2,1,1,1,0.0,36.0,0.,0.,0.,0.
3,1,1,1,0.0,36.0,72.0,0.,0.,0.
4,1,1,1,0.0,0.0,-48.0,0.,0.,0.
1,1,4,1.2E+6,0.187
2,1,2,1.2E+6,0.302
3,1,3,1.2E+6,0.726
```

## Theory

Each 3D truss member contributes a `6×6` stiffness

```
k = (E·A / L) · [  B  -B ]     B = c·cᵀ   (outer product of the
                [ -B   B ]                  direction-cosine vector c)
```

The element matrices are assembled into the global stiffness `K`; the rows/
columns of fixed DOFs are removed; the reduced system `K_ff · d_f = F_f` is
solved by Gaussian elimination with partial pivoting. Reactions are recovered as
`R = K·d − F`, and each member's axial force is `N = (E·A/L)·c·(d_j − d_i)`
(positive = tension).

## JSON output schema

`solve --format json` (and the data embedded in the viewer) emits:

```jsonc
{
  "title": "…", "planar": false, "maxAbsDisplacement": 0.2668,
  "nodes": [
    { "id", "coord":[x,y,z], "fixed":[bool,bool,bool], "load":[…],
      "displacement":[ux,uy,uz], "reaction":[rx,ry,rz] }
  ],
  "elements": [
    { "id", "nodes":[a,b], "length", "axialForce", "stress",
      "strain", "elongation", "state":"tension|compression|zero-force" }
  ]
}
```

## Improvements over the original

- **Implemented the solver.** The legacy `Action/Solve/GetForce` and all matrix
  math were empty; the full direct-stiffness analysis now works end-to-end.
- **Fixed the node-index bug.** The old code used a raw file token as a 0-based
  array index (`GetVertexAt`), so element `1,1,4,…` read out of bounds. Elements
  now resolve nodes by id through a validated id→index map.
- **Robust parsing & validation** with precise English error messages
  (duplicate ids, unknown node refs, zero-length members, non-positive `E`/`A`).
- **Mechanism detection** — a singular reduced matrix is reported as an unstable
  structure instead of producing garbage.
- **New capabilities** — JSON export, an interactive web viewer, a built-in
  server, and a unit-tested engine (`cargo test`).

## Testing

```powershell
cargo test
```

Includes a global-equilibrium check on the classic space-truss example and a
linear-solver correctness/singularity test.

## License

MIT — Kang Taewook (laputa99999@gmail.com)
