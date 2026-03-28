<script lang="ts">
  import { onMount } from "svelte";
  import { EditorView, basicSetup } from "codemirror";
  import { keymap } from "@codemirror/view";
  import { sql } from "@codemirror/lang-sql";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { Prec, Compartment } from "@codemirror/state";

  let {
    value = $bindable(""),
    onRun,
    runRef = $bindable(),
  }: {
    value?: string;
    onRun: (sql: string) => void;
    runRef?: () => void;
  } = $props();

  let editorEl = $state<HTMLDivElement>();
  let view: EditorView;
  let themeCompartment = new Compartment();
  let isComposing = $state(false);

  /**
   * Returns positions of all semicolons that are not inside
   * string literals or comments.
   */
  function semicolonBoundaries(text: string): number[] {
    const result: number[] = [];
    let i = 0;
    while (i < text.length) {
      const ch = text[i];
      const next = text[i + 1];
      // line comment
      if (ch === "-" && next === "-") {
        i += 2;
        while (i < text.length && text[i] !== "\n") i++;
        continue;
      }
      // block comment
      if (ch === "/" && next === "*") {
        i += 2;
        while (i < text.length && !(text[i] === "*" && text[i + 1] === "/"))
          i++;
        i += 2;
        continue;
      }
      // single-quoted string ('' is escaped quote)
      if (ch === "'") {
        i++;
        while (i < text.length) {
          if (text[i] === "'" && text[i + 1] === "'") {
            i += 2;
            continue;
          }
          if (text[i] === "'") {
            i++;
            break;
          }
          i++;
        }
        continue;
      }
      // double-quoted identifier
      if (ch === '"') {
        i++;
        while (i < text.length) {
          if (text[i] === '"' && text[i + 1] === '"') {
            i += 2;
            continue;
          }
          if (text[i] === '"') {
            i++;
            break;
          }
          i++;
        }
        continue;
      }
      if (ch === ";") result.push(i);
      i++;
    }
    return result;
  }

  /**
   * Extracts the SQL statement the cursor is currently inside.
   * Falls back to the full text if only one statement exists.
   */
  function statementAtCursor(text: string, cursor: number): string {
    const bounds = semicolonBoundaries(text);
    if (bounds.length === 0) return text.trim();

    // Walk backward past whitespace so that the gap after a semicolon
    // is treated as belonging to the previous statement.
    let effective = Math.min(cursor, text.length - 1);
    while (effective > 0 && /\s/.test(text[effective])) effective--;

    // Find the semicolon that terminates the statement at the effective position.
    const endIdx = bounds.findIndex((p) => p >= effective);

    if (endIdx === -1) {
      // Effective cursor is past all semicolons (last statement has no trailing semicolon).
      return text.slice(bounds[bounds.length - 1] + 1).trim();
    }

    const endSemi = bounds[endIdx];
    const prevSemi = endIdx > 0 ? bounds[endIdx - 1] : -1;
    return text.slice(prevSemi + 1, endSemi).trim();
  }

  function runAtCursor() {
    const cursor = view.state.selection.main.head;
    const text = view.state.doc.toString();
    onRun(statementAtCursor(text, cursor));
  }

  onMount(() => {
    runRef = runAtCursor;

    // Light theme extension
    const lightTheme = EditorView.theme(
      {
        "&": { background: "transparent !important" },
        ".cm-gutters": {
          background: "transparent !important",
          borderRight: "1px solid rgba(0,0,0,0.1)",
        },
        ".cm-scroller": {
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        },
        ".cm-content": { color: "#1a1a1a" },
        ".cm-cursor": { borderLeftColor: "#1a1a1a" },
      },
      { dark: false },
    );

    // Dark theme extension (using oneDark)
    const darkTheme = EditorView.theme(
      {
        "&": { background: "transparent !important" },
        ".cm-gutters": {
          background: "transparent !important",
          borderRight: "1px solid rgba(255,255,255,0.06)",
        },
        ".cm-scroller": {
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        },
      },
      { dark: true },
    );

    const getThemeExtensions = () => {
      // Check the actual applied theme class on the document
      const isDark = document.documentElement.classList.contains("dark");
      return isDark ? [oneDark, darkTheme] : [lightTheme];
    };

    view = new EditorView({
      doc: value,
      extensions: [
        basicSetup,
        sql(),
        EditorView.lineWrapping,
        themeCompartment.of(getThemeExtensions()),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            value = update.state.doc.toString();
          }
        }),
        Prec.highest(
          keymap.of([
            {
              key: "Ctrl-Enter",
              run() {
                runAtCursor();
                return true;
              },
            },
          ]),
        ),
      ],
      parent: editorEl!,
    });

    // Listen for theme changes by watching the 'dark' class on <html>.
    // Using MutationObserver instead of theme.subscribe() so that system theme
    // changes (which toggle the class but don't change the store value) are also caught.
    const observer = new MutationObserver(() => {
      if (view) {
        view.dispatch({
          effects: themeCompartment.reconfigure(getThemeExtensions()),
        });
      }
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    // Track IME composition to prevent cursor jumps during input method editing
    const handleCompositionStart = () => {
      isComposing = true;
    };
    const handleCompositionEnd = () => {
      isComposing = false;
    };
    editorEl?.addEventListener("compositionstart", handleCompositionStart);
    editorEl?.addEventListener("compositionend", handleCompositionEnd);

    return () => {
      observer.disconnect();
      editorEl?.removeEventListener("compositionstart", handleCompositionStart);
      editorEl?.removeEventListener("compositionend", handleCompositionEnd);
      view?.destroy();
    };
  });

  $effect(() => {
    if (view && !isComposing && value !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
      });
    }
  });
</script>

<div class="h-full overflow-auto text-sm" bind:this={editorEl}></div>
