import React, { createContext, useContext, useState } from "react";
import { decryptWallet, EncryptedWallet } from "../util/wallet";
import { JsWallet } from "../../wasm/dcoin_wasm";

interface WalletContextType {
	activeWallet: JsWallet | null;
	selectWallet: (pubKey: string, ecw: EncryptedWallet, password: string) => Promise<void>;
}

const WalletContext = createContext<WalletContextType | undefined>(undefined);

export const useWallet = (): WalletContextType => {
	const context = useContext(WalletContext);
	if (!context) throw new Error("useWallet must be used within WalletProvider");
	return context;
};
export const WalletProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
	const [activeWallet, setActiveWallet] = useState<JsWallet | null>(null);

	const selectWallet = async (publicKey: string, ecw: EncryptedWallet, password: string): Promise<void> => {
		const wallet = await decryptWallet(publicKey, ecw, password);
		if (wallet) {
			setActiveWallet(wallet);
		}
	};

	return (
		<WalletContext.Provider
			value={{
				activeWallet,
				selectWallet,
			}}
		>
			{children}
		</WalletContext.Provider>
	);
};
