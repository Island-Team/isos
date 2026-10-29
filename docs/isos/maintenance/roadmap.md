# Integration roadmap

[中文](../../../docs_cn/isos/maintenance/roadmap_cn.md)

IsOS 0.1 implements a working manager core. The [platform integration plan](plan.md) separates completed adapters from release gates. The following steps complete the wider Island platform; they are planned interfaces, not features claimed by this release.

1. Port EGIA to the explicit EOS2 contract, keep its IsOS managed headless launch mode, and test both sides against the same request, response, and error examples.
2. Move EOS prelaunch work into IsOS graph service, backend, and skill nodes. Keep the EOS CLI as an attachable client. Migrate old operator endpoints to the versioned local manager API before retiring compatibility wrappers.
3. Publish approved v4 release manifests and matching GitHub Release assets through Island Port, with SHA-256, platform metadata, reproducible build evidence, and install/rollback gates.
4. Add a Python 3.12 external skill worker that can select a configurable interpreter, uv environment, or Conda environment. Use pybind11 within that isolated worker to call the existing native C++ skill ABI.
5. Keep IsOS lifecycle, package selection, policy, and graph validation in Rust. Keep performance-sensitive EOS/EGIA runtime and native skills in C++. Prefer a supervised process boundary and versioned IPC between Rust and C++; use a narrow C ABI only when an in-process call is measured to be necessary. Never exchange C++ objects, Rust ownership, or Python objects across that ABI.
6. Migrate the legacy GECA operator client after EGIA's EOS 2 contract port.

Before a release, run format/lint/tests in each repository, verify a clean install on supported Linux platforms, smoke EOS attach/shutdown and EGIA managed mode, and check English/Chinese task guides and interface examples.
