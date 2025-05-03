// src/wasm-loader.ts
import { Wallet, WalletSchema } from "./wasm-types";
import init, * as wasm from "../../wasm/dcoin_wasm";

let initialized: Promise<typeof wasm> | null = null;

function getWasm(): Promise<typeof wasm> {
	if (!initialized) {
		initialized = init().then(() => wasm);
	}
	return initialized;
}

export async function createWallet(): Promise<Wallet> {
	let wasm = await getWasm();
	const raw = wasm.create_wallet();
	const parsed = WalletSchema.parse(raw);
	return parsed;
}
