import { useEffect } from "react";
import { Wallet } from "./util/wasm/wasm-types";
import { createWallet } from "./util/wasm/wasm";
import { useWallet } from "./context/walletContext";
import { encryptAndStoreWallet, EncryptedWalletMap, getWalletList } from "./util/wallet";

function App() {
	const { selectWallet, activeWallet } = useWallet();

	async function testWallet(wallet: Wallet) {
		await encryptAndStoreWallet(wallet, "testing");
		const wallets: EncryptedWalletMap = await getWalletList();
		const keys = Object.keys(wallets);
		const selectedWallet = keys[0];
		if (keys.length) {
			selectWallet(selectedWallet, wallets[selectedWallet], "testing");
		}
	}

	useEffect(() => {
		createWallet().then((wallet) => {
			testWallet(wallet);
		});
	}, []);

	return (
		<div>
			<h1>WASM Wallet Example</h1>
			{activeWallet ? <pre>{JSON.stringify(activeWallet, null, 2)}</pre> : <p>Loading...</p>}
		</div>
	);
}

export default App;
