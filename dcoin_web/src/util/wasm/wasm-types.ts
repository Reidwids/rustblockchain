import { z } from "zod";

export const WalletSchema = z.object({
	private_key: z.string(),
	public_key: z.string(),
});

export type Wallet = z.infer<typeof WalletSchema>;
