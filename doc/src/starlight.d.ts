// Starlight declares its `virtual:starlight/components/*` modules only in
// `virtual-internal.d.ts`, which consumers cannot reference. Component
// overrides (`src/components/Sidebar.astro`) still need the type.
declare module "virtual:starlight/components/MobileMenuFooter" {
  const MobileMenuFooter: typeof import("@astrojs/starlight/components/MobileMenuFooter.astro").default;
  export default MobileMenuFooter;
}
