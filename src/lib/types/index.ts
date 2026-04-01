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

export interface Connection {
  id: string;
  name: string;
  driver: DbDriver;
  color?: string; // Label color
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
}

export interface QueryTab {
  id: string;
  connectionId: string;
  title: string;
  kind?: "query" | "diagram"; // defaults to "query" when absent
  sql: string;
  result?: QueryResult;
  isLoading: boolean;
  temporary?: boolean; // preview tab opened by single click; replaced on next single click
  // diagram-only
  selectedTables?: string[];
  nodePositions?: Record<string, { x: number; y: number }>;
}
