-- A new chat's wallpaper (ds/wallpaper) under an effect, as its art changes
-- (ui/composer_place_fixture.luau). The art fades in inside its layer over
-- the layer leaving, and a crossfade always ends on the newest art (art
-- once came up blank here as a picture changed, or the theme with it, under
-- Scanlines: zgx now draws a cached layer again once a picture in it has
-- decoded, zgx 2e22d99d). Pixels aren't in a test's reach; this pins the
-- shape that draws them (and
-- src/plugins/examples_tests.rs's Parallax scenes test, that a plugin's
-- `ui.change` still reaches the place: no presence nested round it).

test("wallpaper: new art crossfades over the old and ends on the newest", function()
	local app = mount("composer_place_fixture.luau")
	app:resize(900, 360)
	app:set_state("test_place_effect", "scanlines")
	app:wait("#never-shown", { timeout_ms = 900 })
	expect(app:has("#wallpaper-layer >> #wall-plate-1-night")).to_be(true)
	-- A picture alone: the new fades in over the old, each in its layer.
	app:set_state("test_place_picture", "plate-2-bluehour")
	app:frames(2)
	expect(app:has("#wallpaper-layer >> #wall-plate-1-night")).to_be(true)
	expect(app:has("#wallpaper-layer >> #wall-plate-2-bluehour")).to_be(true)
	app:wait("#never-shown", { timeout_ms = 900 })
	expect(app:count("#wallpaper-layer")).to_be(1)
	expect(app:has("#wall-plate-1-night")).to_be(false)
	expect(app:has("#wallpaper-layer >> #wall-plate-2-bluehour")).to_be(true)
	-- The theme and the picture at once (what came up blank): one layer, on
	-- the newest art.
	app:set_state("test_place_dark", false)
	app:set_state("test_place_picture", "plate-3-sunrise")
	app:frames(1)
	app:set_state("test_place_picture", "plate-1-night")
	app:wait("#never-shown", { timeout_ms = 900 })
	expect(app:count("#wallpaper-layer")).to_be(1)
	expect(app:has("#wallpaper-layer >> #wall-plate-1-night")).to_be(true)
	expect(app:has("#wall-plate-3-sunrise")).to_be(false)
	expect(app:has("#wall-plate-2-bluehour")).to_be(false)
	-- No wallpaper: no layer.
	app:set_state("test_place_art", false)
	app:wait("#never-shown", { timeout_ms = 900 })
	expect(app:count("#wallpaper-layer")).to_be(0)
	expect(app:error()).to_be(nil)
end)

-- A node's rectangle (its first line in the tree under `scope`): x, y, w, h.
local function rect(app: any, scope: string): (number, number, number, number)
	local line: string = (app:tree(scope) :: string):match("[^\n]+") or ""
	local x, y, w, h = line:match(" (%-?[%d.]+),(%-?[%d.]+) ([%d.]+)x([%d.]+)")
	return tonumber(x) or -1, tonumber(y) or -1, tonumber(w) or -1, tonumber(h) or -1
end

test(
	"wallpaper: a new chat's art takes Zeron's room, the sidebar's edge to the window's, the panel's card over it",
	function()
		local app = mount("shell_fixture.luau")
		app:resize(1200, 800)
		app:set_state(
			"shell_wallpaper_image",
			{ path = "assets/art/plate-1-night.jpg", name = "night", w = 2100, h = 700, x = 0.5, y = 0.5, zoom = 1 }
		)
		app:set_state("shell_wallpaper", "image")
		app:set_state("shell_wallpaper_effect", "dither")
		app:frames(40)
		-- From the window's top (under the top bar) and the sidebar's edge to
		-- the window's right, the art in the top 72% of the window's height.
		local sx, _, sw = rect(app, "#shell-sidebar")
		local x, y, w, h = rect(app, "#wallpaper-layer")
		expect(`{x},{y} {w}x{h}`).to_be(`{sx + sw},0 {1200 - sx - sw}x800`)
		local _, ay, _, ah = rect(app, "#wallpaper-art")
		expect(`{ay} {ah}`).to_be(`0 {0.72 * 800}`)
		-- The sidebar folded away: the art covers the window's whole width.
		app:command("shell.toggle_sidebar")
		app:frames(40)
		x, y, w, h = rect(app, "#wallpaper-layer")
		expect(`{x},{y} {w}x{h}`).to_be("0,0 1200x800")
		-- The panel open: its card stands over the art, laid out as it was
		-- (Zeron's right pane clipped its art, never rescaled it).
		app:command("shell.toggle_files")
		app:frames(40)
		expect(app:has("#shell-files")).to_be(true)
		x, y, w, h = rect(app, "#wallpaper-layer")
		expect(`{x},{y} {w}x{h}`).to_be("0,0 1200x800")
		-- A session: no art.
		app:command("shell.toggle_files")
		app:command("shell.toggle_sidebar")
		app:frames(40)
		app:click(`#shell-sidebar >> role:button "Fix the build"`)
		app:wait("#never-shown", { timeout_ms = 600 })
		expect(app:count("#wallpaper-layer")).to_be(0)
		expect(app:error()).to_be(nil)
	end
)
