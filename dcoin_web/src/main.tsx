import { createRoot } from "react-dom/client";
import "./index.css";
import App from "./App.tsx";
import { WalletProvider } from "./context/walletContext.tsx";

createRoot(document.getElementById("root")!).render(
	<WalletProvider>
		<App />
	</WalletProvider>
);
