import { Privacy } from "./Privacy";
import "./layout.css"; // .doc-page styling lives here

// Static content page — pure markup, no data. Import the layout CSS so the
// .doc-page wrapper is styled in the catalog.
const Default = () => <Privacy />;

export default {
  Default,
};
