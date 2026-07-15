import type { Story, StoryDefault } from "@ladle/react";
import { Privacy } from "./Privacy";
import "./layout.css"; // .doc-page styling lives here

// Static content page — pure markup, no data. Import the layout CSS so the
// .doc-page wrapper is styled in the catalog.
export default {
  title: "Pages/Privacy",
} satisfies StoryDefault;

export const Default: Story = () => <Privacy />;
