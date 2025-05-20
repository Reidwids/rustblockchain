import { useEffect } from "react";
import { useWallet } from "./context/walletContext";
import { deleteWallet, encryptAndStoreWallet, getWalletList } from "./util/wallet";
import { createWallet, sendTransaction } from "./util/wasm/wasm";

function App() {
	const { selectWallet, activeWallet } = useWallet();

	async function testWallet() {
		const wallets = await getWalletList();

		const keys = Object.keys(wallets);
		if (keys.length) {
			const selectedWalletPubKey = keys[0];
			selectWallet(selectedWalletPubKey, wallets[selectedWalletPubKey], "testing");
		}
	}

	async function createNewWallet() {
		const wallet = await createWallet();
		const newWallet = await encryptAndStoreWallet(wallet, "testing");
		console.log(newWallet);
	}

	async function deleteAllWallets() {
		const wallets = await getWalletList();
		for (const walletPubKey in wallets) {
			deleteWallet(walletPubKey);
		}
		console.log("Deleted all wallets");
	}

	async function sendtx() {
		if (activeWallet) {
			console.log("TESTING SEND TX from ", activeWallet.get_public_key());

			sendTransaction("test", activeWallet, 1);
		}
	}

	useEffect(() => {
		testWallet();
	}, []);

	return (
		<div className="flex flex-col">
			<h1>WASM Wallet Example</h1>
			{activeWallet ? <pre>{"Active pubKey: " + activeWallet.get_public_key()}</pre> : <p>Loading...</p>}
			<button className="bg-blue-800 m-5 cursor-pointer" onClick={createNewWallet}>
				Create a Wallet
			</button>
			<button className="bg-blue-800 m-5 cursor-pointer" onClick={sendtx}>
				Send a Tx
			</button>
			<button className="bg-blue-800 m-5 cursor-pointer" onClick={deleteAllWallets}>
				Delete all Wallets
			</button>
		</div>
	);
}

export default App;
