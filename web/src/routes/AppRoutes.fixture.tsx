import { MemoryRouter } from "react-router-dom";
import { AppRoutes } from "./AppRoutes";

// The production route composition at the site's default URL.
export default (
  <MemoryRouter initialEntries={["/"]}>
    <AppRoutes />
  </MemoryRouter>
);
