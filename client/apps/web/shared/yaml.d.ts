// A YAML file imported as a module is its parsed contents: Bun parses it on import, and the server's
// bundle is handed the same object by the YAML plugin the i18n module registers for its locale files.
declare module "*.yaml" {
  const value: unknown;
  export default value;
}
