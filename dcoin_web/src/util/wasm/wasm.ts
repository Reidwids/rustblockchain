import init, * as wasm from "../../../wasm/dcoin_wasm";

let initialized: Promise<typeof wasm> | null = null;

function getWasm(): Promise<typeof wasm> {
	if (!initialized) {
		initialized = init().then(() => wasm);
	}
	return initialized;
}

export async function createWallet(): Promise<wasm.JsWallet> {
	let wasm = await getWasm();
	return new wasm.JsWallet();
}

export async function sendTransaction(to: string, from: wasm.JsWallet, value: number): Promise<void> {
	let wasm = await getWasm();
	await wasm.send_tx(to, from, value);
}

export async function getWalletFromKeys(pubKey: string, privKey: string): Promise<wasm.JsWallet> {
	let wasm = await getWasm();
	return wasm.JsWallet.from_keys(pubKey, privKey);
}
