export class SelectedRouteUnavailableError extends Error {
  readonly routeMode: string;

  constructor(key: string, routeMode: string, detail?: string | null) {
    super(`${key}: selected ${routeMode} route did not answer${detail ? `: ${detail}` : ""}`);
    this.name = "SelectedRouteUnavailableError";
    this.routeMode = routeMode;
  }
}
