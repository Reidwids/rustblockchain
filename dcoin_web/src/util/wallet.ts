import { Wallet } from "./wasm/wasm-types";

export type EncryptedWallet = {
	ciphertext: string;
	salt: string;
	iv: string;
};

export type EncryptedWalletMap = {
	[key: string]: EncryptedWallet;
};

const deriveKey = async (password: string, salt: Uint8Array): Promise<CryptoKey> => {
	const enc = new TextEncoder();
	const keyMaterial = await window.crypto.subtle.importKey("raw", enc.encode(password), { name: "PBKDF2" }, false, [
		"deriveKey",
	]);
	return window.crypto.subtle.deriveKey(
		{ name: "PBKDF2", salt, iterations: 100_000, hash: "SHA-256" },
		keyMaterial,
		{ name: "AES-GCM", length: 256 },
		false,
		["encrypt", "decrypt"]
	);
};

export const encryptAndStoreWallet = async (wallet: Wallet, password: string): Promise<void> => {
	const enc = new TextEncoder();
	const salt = window.crypto.getRandomValues(new Uint8Array(16));
	const iv = window.crypto.getRandomValues(new Uint8Array(12));
	const key = await deriveKey(password, salt);

	const ciphertext = await window.crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, enc.encode(wallet.private_key));

	const entry: EncryptedWallet = {
		ciphertext: btoa(String.fromCharCode(...new Uint8Array(ciphertext))),
		salt: btoa(String.fromCharCode(...salt)),
		iv: btoa(String.fromCharCode(...iv)),
	};

	const wallets: EncryptedWalletMap = JSON.parse(localStorage.getItem("wallets") || "{}");
	wallets[wallet.public_key] = entry;
	localStorage.setItem("wallets", JSON.stringify(wallets));
};

export const decryptWallet = async (
	publicKey: string,
	walletEntry: EncryptedWallet,
	password: string
): Promise<Wallet | null> => {
	const salt = Uint8Array.from(atob(walletEntry.salt), (c) => c.charCodeAt(0));
	const iv = Uint8Array.from(atob(walletEntry.iv), (c) => c.charCodeAt(0));
	const ciphertext = Uint8Array.from(atob(walletEntry.ciphertext), (c) => c.charCodeAt(0));

	const key = await deriveKey(password, salt);
	try {
		const decrypted = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ciphertext);
		return {
			public_key: publicKey,
			private_key: new TextDecoder().decode(decrypted),
		};
	} catch {
		return null;
	}
};

export const deleteWallet = (publicKey: string) => {
	localStorage.removeItem(publicKey);
};

export const getWalletList = (): {} => {
	return JSON.parse(localStorage.getItem("wallets") || "{}");
};
