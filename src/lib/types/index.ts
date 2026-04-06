export type DbDriver = "postgres" | "mysql" | "sqlite" | "sqlserver";

export interface SchemaColumn {
  name: string;
  colType: string;
  pk: boolean;
  nullable: boolean;
}

export interface SchemaTable {
  name: string;
  columns: SchemaColumn[];
}

export interface SchemaForeignKey {
  name: string;
  fromTable: string;
  fromCol: string;
  toTable: string;
  toCol: string;
}

export interface SchemaGraph {
  tables: SchemaTable[];
  foreignKeys: SchemaForeignKey[];
}

export interface SshTunnel {
  host: string;
  port: number;
  username: string;
  password?: string;
  privateKey?: string;
  privateKeyPassphrase?: string;
}

export interface QueryHistoryEntry {
  sql: string;
  timestamp: string; // ISO string
  success: boolean;
  error?: string;
  rowsAffected?: number;
}

export interface Connection {
  id: string;
  name: string;
  driver: DbDriver;
  color?: string; // Label color
  locked?: boolean; // When true, only SELECT queries are allowed
  queryHistory?: QueryHistoryEntry[]; // Last 1000 executed queries
  host?: string;
  port?: number;
  database: string;
  username?: string;
  password?: string;
  filePath?: string; // for SQLite
  ssh?: SshTunnel; // SSH tunnel configuration
  sslMode?: string; // postgres: disable | allow | prefer | require | verify-ca | verify-full
}

export interface TableInfo {
  name: string;
  kind: "table" | "view";
}

export interface QueryResult {
  columns: string[];
  columnTypes?: string[];
  columnNullable?: boolean[];
  rows: Record<string, unknown>[];
  rowsAffected?: number;
  error?: string;
  queryKey?: string; // unique per execution; used to detect new queries vs. streaming row appends
}

export type StreamUpdate =
  | { type: "header"; columns: string[]; columnTypes: string[]; columnNullable: boolean[] }
  | { type: "rows"; rows: Record<string, unknown>[] }
  | { type: "done"; rowsAffected?: number }
  | { type: "cancelled" }
  | { type: "error"; message: string };

export interface ServerInfo {
  version?: string;
  databaseName?: string;
  connections?: number;
  size?: string;
  host?: string;
  port?: number;
  uptime?: string;
  extra: [string, string][];
}

export type ExportProgress =
  | { type: "started"; totalTables: number }
  | { type: "table"; name: string; index: number; total: number }
  | { type: "done" }
  | { type: "error"; message: string };

export type ImportProgress =
  | { type: "progress"; done: number; total: number }
  | { type: "done"; count: number }
  | { type: "error"; message: string };

export interface QueryTab {
  id: string;
  connectionId: string;
  title: string;
  kind?: "query" | "diagram"; // defaults to "query" when absent
  sql: string;
  lastExecutedSql?: string; // The last executed query (for refreshing after updates)
  result?: QueryResult;
  isLoading: boolean;
  temporary?: boolean; // preview tab opened by single click; replaced on next single click
  autoRun?: boolean;  // run the default query immediately when the tab first opens
  // diagram-only
  selectedTables?: string[];
  nodePositions?: Record<string, { x: number; y: number }>;
}
