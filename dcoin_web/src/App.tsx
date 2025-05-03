import { useEffect, useState } from "react";
import { createWallet } from "./wasm-util/wasm";
import { Wallet } from "./wasm-util/wasm-types";

function App() {
	const [wallet, setWallet] = useState<Wallet>();

	useEffect(() => {
		createWallet().then((wallet) => {
			setWallet(wallet);
		});
	}, []);

	return (
		<div>
			<h1>WASM Wallet Example</h1>
			{wallet ? <pre>{JSON.stringify(wallet, null, 2)}</pre> : <p>Loading WASM...</p>}
		</div>
	);
}

export default App;
