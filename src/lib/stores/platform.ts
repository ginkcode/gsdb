import { writable } from "svelte/store";
import { browser } from "$app/environment";
import { platform as getOsPlatform } from "@tauri-apps/plugin-os";
import { isTauri } from "@tauri-apps/api/core";

export type Platform = "windows" | "macos" | "linux" | "unknown";

function createPlatformStore() {
	const { subscribe, set } = writable<Platform>("unknown");
	let initialized = false;

	async function detectPlatform(): Promise<Platform> {
		if (!browser) return "unknown";

		// Check if running in Tauri using the official API
		if (isTauri()) {
			try {
				const osPlatform = await getOsPlatform();
				console.log("[platform] Detected OS:", osPlatform);
				if (osPlatform === "windows") return "windows";
				if (osPlatform === "macos") return "macos";
				if (osPlatform === "linux") return "linux";
			} catch (err) {
				console.error("[platform] OS plugin error:", err);
			}
		} else {
			console.log("[platform] Not running in Tauri");
		}

		return "unknown";
	}

	async function initialize() {
		if (initialized) return;
		initialized = true;
		const detected = await detectPlatform();
		set(detected);
	}

	return {
		subscribe,
		initialize,
		refresh: async () => {
			const detected = await detectPlatform();
			set(detected);
		}
	};
}

export const platform = createPlatformStore();

// Helper to get border radius class based on platform
export function getBorderRadiusClass(platform: Platform): string {
	if (platform === "windows") {
		return "rounded-md";
	}
	return "rounded-xl";
}