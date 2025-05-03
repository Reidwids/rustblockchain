import { useEffect, useState } from "react";
import { loadWasm } from "./wasm-loader";

function App() {
	const [wallet, setWallet] = useState<any>(null);

	useEffect(() => {
		loadWasm().then(({ create_wallet }) => {
			const result = create_wallet();
			setWallet(result);
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
