# Tether UI Style Guide

This document defines the visual design language and component patterns for the Tether application.

## Design Philosophy

Tether's UI follows these core principles:
- **Clean & Minimal** - Reduce visual noise, focus on content
- **Professional** - Understated, functional, trustworthy
- **Consistent** - Predictable patterns across all interfaces
- **Fast** - Lightweight, snappy interactions

## Color Palette

### Neutrals (Primary)
Tether uses a grayscale palette as the foundation:

- **Background**: `bg-white`, `bg-gray-50`
- **Borders**: `border-gray-200`, `border-gray-300`
- **Text Primary**: `text-gray-900`
- **Text Secondary**: `text-gray-700`
- **Text Tertiary**: `text-gray-600`, `text-gray-500`
- **Text Disabled**: `text-gray-400`
- **Hover States**: `hover:bg-gray-100`, `hover:bg-gray-50`

### Accent Colors (Semantic)
Use sparingly for specific purposes:

- **Primary/Action**: `bg-blue-600`, `text-blue-600`, `border-blue-500`
  - Hover: `hover:bg-blue-700`
  - Light variant: `bg-blue-50`, `text-blue-900`
- **Success/Ready**: `bg-emerald-50`, `text-emerald-700`
- **Warning/Pending**: `bg-amber-50`, `text-amber-700`, `bg-yellow-50`
- **Error/Danger**: `bg-red-50`, `text-red-700`, `border-red-200`
- **Secrets/Security**: `bg-purple-50`, `text-purple-700`
- **Admin Mode**: `bg-orange-50`, `text-orange-700`, `border-orange-300`

### Avoid
- Heavy gradients
- Saturated/bright colors
- Multiple accent colors in one component
- Dark backgrounds (except for code)

## Typography

### Font Stack
- **Body**: System default (Tailwind's font-sans)
- **Code**: `font-mono` (Monaco, Menlo, Courier)

### Text Sizes
- **Base**: `text-sm` (0.875rem) - Default for most text
- **Small**: `text-xs` (0.75rem) - Labels, hints, metadata
- **Medium**: `text-base` (1rem) - Emphasis, titles
- **Large**: `text-lg` (1.125rem) - Section headers
- **XL**: `text-xl` (1.25rem) - Page titles

### Font Weights
- **Normal**: `font-normal` - Body text (rare, most text is medium)
- **Medium**: `font-medium` - Default for most text
- **Semibold**: `font-semibold` - Headers, emphasis
- **Bold**: `font-bold` - Avoid unless necessary

### Text Colors
Follow the neutral palette above. Default to `text-gray-700` or `text-gray-900`.

## Spacing

Use Tailwind's spacing scale consistently:
- **Tight**: `gap-1` (4px), `gap-1.5` (6px), `gap-2` (8px)
- **Normal**: `gap-3` (12px), `gap-4` (16px)
- **Loose**: `gap-6` (24px), `gap-8` (32px)

### Padding
- **Compact**: `px-2 py-1`, `px-3 py-2`
- **Normal**: `px-4 py-3`, `px-6 py-4`
- **Spacious**: `px-6 py-6`, `px-8 py-8`

### Margin
- Use `mb-2`, `mb-4` for vertical rhythm
- Prefer `gap` in flex/grid layouts over margins

## Components

### Buttons

#### Primary Button
```jsx
<button className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors shadow-sm">
  Primary Action
</button>
```

#### Secondary Button
```jsx
<button className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all">
  Secondary Action
</button>
```

#### Danger Button
```jsx
<button className="px-3 py-1.5 text-sm font-medium text-red-700 bg-white hover:bg-red-50 border border-red-300 rounded-md transition-all">
  Delete
</button>
```

#### Disabled State
```jsx
disabled={true}
className="... disabled:opacity-50 disabled:cursor-not-allowed"
```

### Inputs

#### Text Input
```jsx
<input
  type="text"
  className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
  placeholder="Enter text..."
/>
```

#### Search Input
```jsx
<input
  type="text"
  className="flex-1 px-4 py-2 text-sm border border-gray-200 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
  placeholder="Search..."
/>
```

### Cards & Containers

#### Card
```jsx
<div className="bg-white border border-gray-200 rounded-lg p-4">
  Card content
</div>
```

#### Section Container
```jsx
<div className="px-6 py-4 border-b border-gray-200 bg-white">
  Section content
</div>
```

### Status Indicators

#### Badge
```jsx
<span className="text-xs px-2 py-1 rounded-md font-medium bg-blue-50 text-blue-700">
  Status
</span>
```

#### Dot Indicator
```jsx
<span className="inline-block w-2 h-2 rounded-full bg-emerald-500"></span>
```

### Lists & Tables

#### List Item (Active/Inactive)
```jsx
<div className={`px-3 py-2 rounded cursor-pointer transition-all ${
  isActive
    ? 'bg-blue-50 text-blue-900 font-medium border-l-2 border-blue-500'
    : 'text-gray-700 hover:bg-white'
}`}>
  List item
</div>
```

#### Table Headers
```jsx
<th className="text-left px-4 py-3 bg-gray-50 text-gray-700 text-xs font-semibold uppercase tracking-wider border-b border-gray-200">
  Header
</th>
```

### Dialogs & Modals

#### Overlay
```jsx
<div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50" onClick={onClose}>
  {/* Dialog content */}
</div>
```

#### Dialog Box
```jsx
<div className="bg-white rounded-lg shadow-xl max-w-md w-full p-6" onClick={(e) => e.stopPropagation()}>
  <h3 className="text-lg font-semibold text-gray-900 mb-4">Dialog Title</h3>
  {/* Content */}
  <div className="flex gap-2 mt-6">
    <button className="flex-1 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md">
      Confirm
    </button>
    <button className="flex-1 px-4 py-2 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md">
      Cancel
    </button>
  </div>
</div>
```

### Authentication Gates

Use minimal, centered layout with clean messaging:

```jsx
<div className="flex-1 flex items-center justify-center p-8 bg-gray-50">
  <div className="text-center max-w-sm">
    <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-blue-100 text-blue-600 text-3xl mb-6">
      🔐
    </div>
    <h3 className="text-lg font-semibold text-gray-900 mb-2">Authentication Required</h3>
    <p className="text-sm text-gray-600 mb-6 leading-relaxed">
      Description of why authentication is needed and what it protects.
    </p>
    <button className="w-full px-4 py-3 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors">
      Authenticate
    </button>
  </div>
</div>
```

### Error Messages

```jsx
<div className="bg-red-50 border border-red-200 rounded-lg px-4 py-3 text-red-800 text-sm">
  <p className="font-medium">Error occurred</p>
  <p className="text-xs mt-1 text-red-600">Detailed error message</p>
</div>
```

### Empty States

```jsx
<div className="flex flex-col items-center justify-center py-12 text-center">
  <div className="text-5xl mb-4">📓</div>
  <h3 className="text-base font-semibold text-gray-900 mb-2">No items yet</h3>
  <p className="text-sm text-gray-600 mb-6">Get started by creating your first item.</p>
  <button className="px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md">
    Create Item
  </button>
</div>
```

## Icons & Emojis

### Usage
- Use emojis sparingly, primarily for visual hierarchy in lists
- Size emojis consistently: `text-sm` or `text-base` for inline, `text-3xl` to `text-5xl` for large display

### Common Icons
- **Workbook**: 📓
- **File**: 📄
- **Folder**: 📁
- **Security**: 🔐
- **Schedule**: ⏰
- **Settings**: ⚙️
- **Success**: ✓
- **Error**: ⚠
- **Close**: ✕

## Animations & Transitions

### Standard Transition
```jsx
className="transition-colors"  // For color changes
className="transition-all"     // For multiple properties
```

### Hover States
Always include smooth transitions on interactive elements.

### Loading States
```jsx
// Subtle pulse for busy indicators
className="animate-pulse-subtle"

// Spinner (use sparingly)
<div className="inline-block w-4 h-4 border-2 border-gray-300 border-t-blue-600 rounded-full animate-spin"></div>
```

## Border Radius

- **Small**: `rounded` (0.25rem) - Badges, small buttons
- **Medium**: `rounded-md` (0.375rem) - Buttons, inputs
- **Large**: `rounded-lg` (0.5rem) - Cards, dialogs
- **Circle**: `rounded-full` - Avatar, badges, status dots

## Shadows

Use shadows sparingly:
- **Subtle**: `shadow-sm` - Slight elevation
- **Standard**: `shadow` - Default card shadow
- **Elevated**: `shadow-lg` - Dialogs, modals
- **Emphasis**: `shadow-xl` - Highest elevation

Avoid using shadows on flat UI elements like list items or inline content.

## Layout Patterns

### Full-Height Containers
```jsx
<div className="flex flex-col h-full">
  <header className="flex-shrink-0">...</header>
  <main className="flex-1 overflow-auto">...</main>
  <footer className="flex-shrink-0">...</footer>
</div>
```

### Sidebar Layout
```jsx
<div className="flex h-screen">
  <aside className="w-64 border-r border-gray-200 bg-gray-50 overflow-y-auto">
    Sidebar
  </aside>
  <main className="flex-1 overflow-auto bg-white">
    Content
  </main>
</div>
```

### Centered Content
```jsx
<div className="flex items-center justify-center h-full">
  <div className="max-w-md w-full">
    Centered content
  </div>
</div>
```

## Responsive Design

Tether is primarily a desktop application. Use fixed layouts where appropriate rather than fluid responsive patterns.

## Accessibility

- Always include hover states for interactive elements
- Use semantic color coding (red = danger, green = success)
- Provide `title` attributes for icon-only buttons
- Ensure sufficient contrast (WCAG AA minimum)
- Use `disabled` attribute for disabled buttons

## Anti-Patterns to Avoid

❌ Heavy gradients or shadows
❌ Bright, saturated colors
❌ Mixed design systems (don't use both Bootstrap and Tailwind patterns)
❌ Inconsistent spacing (stick to the scale)
❌ Large emojis as primary UI elements
❌ Complex animations or transitions
❌ Mixing font sizes within a single text block
❌ Using color alone to convey meaning

## Code Style

### JSX Structure
```jsx
// Good: Clear hierarchy, consistent spacing
<div className="flex items-center gap-2">
  <span className="text-sm text-gray-700">Label</span>
  <button className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-md">
    Action
  </button>
</div>

// Bad: Inconsistent, hard to read
<div className="flex gap-4">
  <span className="text-gray-700 text-xs">Label</span>
  <button className="bg-blue-500 px-2 py-1 rounded text-white">Action</button>
</div>
```

### Conditional Styling
```jsx
// Good: Template literal with clear conditions
className={`px-3 py-2 rounded ${
  isActive
    ? 'bg-blue-50 text-blue-900 font-medium'
    : 'text-gray-700 hover:bg-gray-50'
}`}

// Bad: String concatenation, hard to read
className={"px-3 py-2 rounded " + (isActive ? "bg-blue-50" : "bg-white")}
```

## Design Review Checklist

Before shipping a new component:
- [ ] Uses approved color palette (grays + blue accents)
- [ ] Follows spacing scale (no arbitrary values)
- [ ] Includes hover states for interactive elements
- [ ] Uses appropriate text sizes (text-xs, text-sm, text-base)
- [ ] Has consistent border radius with other components
- [ ] Matches existing component patterns where applicable
- [ ] No heavy shadows or gradients
- [ ] Clean, minimal aesthetic
- [ ] Works well in light mode (Tether doesn't have dark mode)
- [ ] Respects the app's professional, understated tone
