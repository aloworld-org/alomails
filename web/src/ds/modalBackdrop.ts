/** One backdrop treatment for every blocking popup surface. The blur strength
 * is a design token so modal implementations cannot drift between apps. */
export const MODAL_BACKDROP_CLASS =
  "backdrop-blur-[var(--overlay-backdrop-blur)]";
