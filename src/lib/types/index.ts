export type DbDriver = "postgres" | "mysql" | "sqlite";

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
  rows: Record<string, unknown>[];
  rowsAffected?: number;
  error?: string;
}

export interface QueryTab {
  id: string;
  connectionId: string;
  title: string;
  sql: string;
  result?: QueryResult;
  isLoading: boolean;
  temporary?: boolean; // preview tab opened by single click; replaced on next single click
}
