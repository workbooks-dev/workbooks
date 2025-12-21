# Marketplace - Design Questions

Questions to answer before implementation:

## 1. Multiple Notebooks Per Template

Should a template support multiple notebooks?
- Example: "ETL Pipeline" with load.ipynb, transform.ipynb, export.ipynb
- Or keep it simple: one template = one notebook for v1?

**Decision:** _TBD_

---

## 2. Installation Location

Where do template notebooks get saved in the project?
- Root directory?
- `notebooks/` subfolder?
- User chooses location?
- What if a notebook with that name already exists?

**Decision:** _TBD_

---

## 3. Dependency Conflicts

What if template needs `pandas==2.0.0` but project has `pandas==1.5.0`?
- Just let uv resolve conflicts automatically?
- Show warning/diff before merging?
- Let user review and approve?

**Decision:** _TBD_

---

## 4. README Rendering

How should template README be displayed?
- Render markdown in app?
- Show as plain text?
- Just have a "View on GitHub" link?

**Decision:** _TBD_

---

## 5. Template Caching/Refresh

How should template index be cached and refreshed?
- Cache template index on first fetch for offline use
- How to refresh? Manual button? Auto on app start?
- Show "last updated" timestamp?

**Decision:** _TBD_

---

## 6. Template Repositories

When and how should we create the template repos?
- Create `tether-dev/templates-official` and `tether-dev/templates-community` now?
- Wait until UI is ready?
- What should the initial repo structure look like?
- Any example templates to include initially?

**Decision:** _TBD_
