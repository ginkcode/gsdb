<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import {
    SvelteFlow,
    Background,
    MiniMap,
    Controls,
    type Node,
    type Edge,
  } from "@xyflow/svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { writeFile, writeTextFile } from "@tauri-apps/plugin-fs";
  import { downloadDir } from "@tauri-apps/api/path";
  import { toast } from "svelte-sonner";
  import { updateTab } from "$lib/stores/connections";
  import type { SchemaGraph, SchemaTable } from "$lib/types";
  import DiagramTableNode from "./DiagramTableNode.svelte";

  let {
    tabId,
    connectionId,
    selectedTables,
    nodePositions,
  }: {
    tabId: string;
    connectionId: string;
    selectedTables: string[];
    nodePositions: Record<string, { x: number; y: number }>;
  } = $props();

  const nodeTypes = { table: DiagramTableNode };

  let schema = $state<SchemaGraph | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Build xyflow nodes from schema + positions
  function buildNodes(graph: SchemaGraph, selected: string[]): Node[] {
    const filtered = graph.tables.filter((t) => selected.includes(t.name));
    const COLS = Math.max(1, Math.ceil(Math.sqrt(filtered.length)));
    const COL_W = 280;
    const ROW_H = 220;

    return filtered.map((table: SchemaTable, i: number) => ({
      id: table.name,
      type: "table",
      position: nodePositions[table.name] ?? {
        x: (i % COLS) * COL_W,
        y: Math.floor(i / COLS) * ROW_H,
      },
      data: { table },
    }));
  }

  // Build xyflow edges from foreign keys
  function buildEdges(graph: SchemaGraph, selected: string[]): Edge[] {
    return graph.foreignKeys
      .filter(
        (fk) =>
          selected.includes(fk.fromTable) && selected.includes(fk.toTable),
      )
      .map((fk) => ({
        id: fk.name,
        source: fk.fromTable,
        sourceHandle: `${fk.fromTable}-${fk.fromCol}-right`,
        target: fk.toTable,
        targetHandle: `${fk.toTable}-${fk.toCol}-left`,
        type: "smoothstep",
        style: "stroke-width: 1.5;",
      }));
  }

  let nodes = $state<Node[]>([]);
  let edges = $state<Edge[]>([]);

  onMount(async () => {
    try {
      schema = await invoke<SchemaGraph>("get_schema", { connectionId });
      nodes = buildNodes(schema, selectedTables);
      edges = buildEdges(schema, selectedTables);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  // Update nodes/edges when selectedTables changes
  $effect(() => {
    if (schema && selectedTables) {
      nodes = buildNodes(schema, selectedTables);
      edges = buildEdges(schema, selectedTables);
    }
  });

  // Export helpers
  const PAD = 40;
  const NODE_W = 240;
  const HEADER_H = 34;
  const ROW_H = 24;
  const RADIUS = 6;

  function xmlEsc(s: string): string {
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function buildDiagramSvg(): string {
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
    for (const node of nodes) {
      const h =
        HEADER_H + (node.data.table as SchemaTable).columns.length * ROW_H;
      minX = Math.min(minX, node.position.x);
      minY = Math.min(minY, node.position.y);
      maxX = Math.max(maxX, node.position.x + NODE_W);
      maxY = Math.max(maxY, node.position.y + h);
    }
    if (!isFinite(minX)) {
      minX = 0;
      minY = 0;
      maxX = 400;
      maxY = 200;
    }

    const W = maxX - minX + PAD * 2;
    const H = maxY - minY + PAD * 2;
    const ox = -minX + PAD;
    const oy = -minY + PAD;
    const parts: string[] = [];

    parts.push(`<rect width="${W}" height="${H}" fill="#ffffff"/>`);

    // Edges - draw stepped lines to match SvelteFlow's "smoothstep" edge type with rounded corners
    for (const edge of edges) {
      const src = nodes.find((n) => n.id === edge.source);
      const tgt = nodes.find((n) => n.id === edge.target);
      if (!src || !tgt) continue;

      // Find column positions for the foreign key relationship
      const srcTable = src.data.table as SchemaTable;
      const tgtTable = tgt.data.table as SchemaTable;

      // Get column names from handle IDs (format: "tableName-colName-side")
      const srcColName =
        edge.sourceHandle?.split("-").slice(1, -1).join("-") || "";
      const tgtColName =
        edge.targetHandle?.split("-").slice(1, -1).join("-") || "";

      // Find column indices
      const srcColIndex = srcTable.columns.findIndex(
        (c) => c.name === srcColName,
      );
      const tgtColIndex = tgtTable.columns.findIndex(
        (c) => c.name === tgtColName,
      );

      // Calculate Y positions based on column indices
      const srcY =
        src.position.y +
        HEADER_H +
        (srcColIndex >= 0 ? srcColIndex + 0.5 : srcTable.columns.length / 2) *
          ROW_H;
      const tgtY =
        tgt.position.y +
        HEADER_H +
        (tgtColIndex >= 0 ? tgtColIndex + 0.5 : tgtTable.columns.length / 2) *
          ROW_H;

      const x1 = src.position.x + NODE_W + ox;
      const y1 = srcY + oy;
      const x2 = tgt.position.x + ox;
      const y2 = tgtY + oy;

      // Draw stepped line with rounded corners using bezier curves
      const midX = (x1 + x2) / 2;
      const cornerRadius = 8; // Match SvelteFlow's default smoothstep radius

      // Path: start -> horizontal -> rounded corner -> vertical -> rounded corner -> horizontal -> end
      let path: string;
      if (x1 < x2) {
        // Source is to the left of target
        path = `M${x1},${y1} L${midX - cornerRadius},${y1} Q${midX},${y1} ${midX},${y1 + Math.sign(y2 - y1) * cornerRadius} L${midX},${y2 - Math.sign(y2 - y1) * cornerRadius} Q${midX},${y2} ${midX + cornerRadius},${y2} L${x2},${y2}`;
      } else {
        // Source is to the right of target
        path = `M${x1},${y1} L${midX + cornerRadius},${y1} Q${midX},${y1} ${midX},${y1 + Math.sign(y2 - y1) * cornerRadius} L${midX},${y2 - Math.sign(y2 - y1) * cornerRadius} Q${midX},${y2} ${midX - cornerRadius},${y2} L${x2},${y2}`;
      }
      parts.push(
        `<path d="${path}" fill="none" stroke="#cbd5e1" stroke-width="1.5"/>`,
      );
    }

    // Nodes
    for (const node of nodes) {
      const table = node.data.table as SchemaTable;
      const x = node.position.x + ox;
      const y = node.position.y + oy;
      const h = HEADER_H + table.columns.length * ROW_H;

      parts.push(
        `<rect x="${x}" y="${y}" width="${NODE_W}" height="${h}" rx="${RADIUS}" fill="#ffffff" stroke="#e2e8f0" stroke-width="1"/>`,
      );
      parts.push(
        `<rect x="${x}" y="${y}" width="${NODE_W}" height="${HEADER_H}" rx="${RADIUS}" fill="#f8fafc"/>`,
      );
      parts.push(
        `<rect x="${x}" y="${y + HEADER_H - RADIUS}" width="${NODE_W}" height="${RADIUS}" fill="#f8fafc"/>`,
      );
      parts.push(
        `<line x1="${x}" y1="${y + HEADER_H}" x2="${x + NODE_W}" y2="${y + HEADER_H}" stroke="#e2e8f0" stroke-width="1"/>`,
      );
      parts.push(
        `<text x="${x + 10}" y="${y + HEADER_H - 10}" font-family="system-ui,sans-serif" font-size="12" font-weight="600" fill="#0f172a">${xmlEsc(table.name)}</text>`,
      );

      for (let i = 0; i < table.columns.length; i++) {
        const col = table.columns[i];
        const cy = y + HEADER_H + i * ROW_H;
        if (i < table.columns.length - 1) {
          parts.push(
            `<line x1="${x}" y1="${cy + ROW_H}" x2="${x + NODE_W}" y2="${cy + ROW_H}" stroke="#f1f5f9" stroke-width="1"/>`,
          );
        }
        if (col.pk) {
          parts.push(
            `<text x="${x + 8}" y="${cy + ROW_H - 7}" font-family="monospace" font-size="9" font-weight="bold" fill="#ca8a04">PK</text>`,
          );
        } else if (col.nullable) {
          parts.push(
            `<text x="${x + 8}" y="${cy + ROW_H - 7}" font-family="monospace" font-size="10" font-weight="bold" fill="#94a3b8">?</text>`,
          );
        }
        const nameX = col.pk || col.nullable ? x + 26 : x + 10;
        parts.push(
          `<text x="${nameX}" y="${cy + ROW_H - 7}" font-family="system-ui,sans-serif" font-size="11" fill="#334155">${xmlEsc(col.name)}</text>`,
        );
        parts.push(
          `<text x="${x + NODE_W - 8}" y="${cy + ROW_H - 7}" font-family="monospace" font-size="10" fill="#ea580c" text-anchor="end">${xmlEsc(col.colType)}</text>`,
        );
      }
    }

    return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">${parts.join("")}</svg>`;
  }

  async function exportSvg() {
    const downloads = await downloadDir();
    const filePath = await save({
      defaultPath: downloads ? `${downloads}/diagram.svg` : "diagram.svg",
      filters: [{ name: "SVG", extensions: ["svg"] }],
    });
    if (!filePath) return;
    try {
      await writeTextFile(filePath, buildDiagramSvg());
      toast.success("Diagram exported as SVG");
    } catch (e) {
      toast.error(`Export failed: ${e}`);
    }
  }

  async function exportPng() {
    const downloads = await downloadDir();
    const filePath = await save({
      defaultPath: downloads ? `${downloads}/diagram.png` : "diagram.png",
      filters: [{ name: "PNG", extensions: ["png"] }],
    });
    if (!filePath) return;
    try {
      const svg = buildDiagramSvg();
      const svgBlob = new Blob([svg], { type: "image/svg+xml" });
      const url = URL.createObjectURL(svgBlob);
      const img = new Image();
      await new Promise<void>((resolve, reject) => {
        img.onload = () => resolve();
        img.onerror = reject;
        img.src = url;
      });
      URL.revokeObjectURL(url);
      const dpr = window.devicePixelRatio || 1;
      const canvas = document.createElement("canvas");
      canvas.width = img.naturalWidth * dpr;
      canvas.height = img.naturalHeight * dpr;
      const ctx = canvas.getContext("2d")!;
      ctx.scale(dpr, dpr);
      ctx.drawImage(img, 0, 0);
      const dataUrl = canvas.toDataURL("image/png");
      const bytes = Uint8Array.from(atob(dataUrl.split(",")[1]), (c) =>
        c.charCodeAt(0),
      );
      await writeFile(filePath, bytes);
      toast.success("Diagram exported as PNG");
    } catch (e) {
      toast.error(`Export failed: ${e}`);
    }
  }

  // Save positions when a node is dragged
  function onNodeDragStop({
    targetNode,
  }: {
    targetNode: Node | null;
    nodes: Node[];
    event: MouseEvent | TouchEvent;
  }) {
    if (!targetNode) return;
    const updated = { ...nodePositions, [targetNode.id]: targetNode.position };
    updateTab(tabId, { nodePositions: updated });
  }
</script>

<div
  class="w-full h-full relative bg-background"
  style="--xy-edge-stroke-default: hsl(var(--muted-foreground)); --xy-edge-stroke-selected-default: hsl(var(--primary)); --xy-edge-label-color: hsl(var(--muted-foreground)); --xy-edge-label-background-color: hsl(var(--background));"
>
  {#if loading}
    <div
      class="flex items-center justify-center h-full gap-2 text-sm text-muted-foreground"
    >
      <span class="w-2 h-2 rounded-full bg-primary animate-bounce"></span>
      Loading schema…
    </div>
  {:else if error}
    <div
      class="flex items-center justify-center h-full text-sm text-destructive"
    >
      {error}
    </div>
  {:else}
    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      fitView
      onnodedragstop={onNodeDragStop}
    >
      <Background patternColor="hsl(var(--border))" gap={20} />
      <MiniMap
        nodeColor="hsl(var(--muted))"
        maskColor="hsl(var(--background) / 0.7)"
        class="minimap"
      />
      <Controls />
    </SvelteFlow>
    <!-- Export buttons -->
    <div class="absolute top-3 right-3 z-10 flex gap-1">
      <button
        onclick={exportSvg}
        class="px-2 py-1 text-xs font-medium rounded border bg-white text-gray-700 border-gray-200 hover:bg-gray-100 transition-colors shadow-sm"
        title="Export as SVG"
      >
        SVG
      </button>
      <button
        onclick={exportPng}
        class="px-2 py-1 text-xs font-medium rounded border bg-white text-gray-700 border-gray-200 hover:bg-gray-100 transition-colors shadow-sm"
        title="Export as PNG"
      >
        PNG
      </button>
    </div>
  {/if}
</div>
