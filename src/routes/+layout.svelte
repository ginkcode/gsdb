<script lang="ts">
    import "./layout.css";
    import { theme, getSystemTheme } from "$lib/stores/theme";
    import { onMount } from "svelte";

    const { children } = $props();

    let systemTheme: "light" | "dark" = "light";
    let currentTheme: "light" | "dark" | "system" = "system";

    function applyTheme(themeValue: "light" | "dark" | "system") {
        const isDark =
            themeValue === "dark" ||
            (themeValue === "system" && systemTheme === "dark");

        document.documentElement.classList.toggle("dark", isDark);
    }

    onMount(() => {
        // Get initial system theme from Tauri
        getSystemTheme().then((detectedTheme) => {
            systemTheme = detectedTheme;
            applyTheme(currentTheme);
        });

        // Subscribe to theme store changes
        const unsubscribe = theme.subscribe((t) => {
            currentTheme = t;
            applyTheme(t);
        });

        // Poll for system theme changes (Tauri's onThemeChanged doesn't work reliably)
        // Check every 2 seconds when "system" theme is selected
        let pollInterval: ReturnType<typeof setInterval> | undefined;

        const startPolling = () => {
            if (pollInterval) clearInterval(pollInterval);
            pollInterval = setInterval(async () => {
                try {
                    const detected = await getSystemTheme();
                    if (detected !== systemTheme) {
                        systemTheme = detected;
                        theme.setSystemTheme(systemTheme);
                        if (currentTheme === "system") {
                            applyTheme("system");
                        }
                    }
                } catch {
                    // Ignore errors
                }
            }, 2000);
        };

        // Start polling immediately
        startPolling();

        return () => {
            unsubscribe();
            if (pollInterval) clearInterval(pollInterval);
        };
    });
</script>

{@render children()}
