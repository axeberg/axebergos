// Run axeberg-cli WASM with Node.js WASI
import { WASI } from 'node:wasi';
import { readFile } from 'node:fs/promises';
import { argv } from 'node:process';

const wasi = new WASI({
  version: 'preview1',
  args: argv.slice(1),
  env: process.env,
  preopens: {
    '.': '.',
  },
});

const wasm = await WebAssembly.compile(
  await readFile('./target/wasm32-wasip1/debug/axeberg-cli.wasm')
);

const instance = await WebAssembly.instantiate(wasm, wasi.getImportObject());
wasi.start(instance);
