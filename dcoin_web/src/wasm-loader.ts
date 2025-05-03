// src/wasm-loader.ts
import init, { create_wallet } from "../wasm";

export async function loadWasm() {
	await init(); // Loads the .wasm binary behind the scenes
	return { create_wallet };
}
