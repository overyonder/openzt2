"""Edit OpenZT2 models in Blender and see each change in the running game.

In OpenZT2, turn on Modder tools in the mod manager, then click the Blender
button below pause. Here, Connect keeps a link to the game and shows what is
selected there. Load opens the selected object as a collection, with one
collection per model. Sync, or saving with Sync on save ticked, sends every
loaded model back to the game, which swaps it in immediately. Export as Mod
packs every model sent this session into a mod that loads from the next start.
"""

import json
import os
import tempfile
import urllib.error
import urllib.parse
import urllib.request

import bpy
from bpy.app.handlers import persistent
from mathutils import Matrix

LINK_ADDRESS = "http://127.0.0.1:37002"
POLL_SECONDS = 1.0
PLACEMENT_NODE = "openzt2_placement"


class LinkError(Exception):
    pass


def post(path, body, content_type, timeout):
    request = urllib.request.Request(
        LINK_ADDRESS + path,
        data=body,
        headers={"Content-Type": content_type},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            reply = json.load(response)
    except (urllib.error.URLError, OSError) as error:
        raise LinkError(
            "OpenZT2 is not listening. Turn on Modder tools and click the "
            "Blender button below pause."
        ) from error
    if "error" in reply:
        raise LinkError(reply["error"].get("message", "OpenZT2 refused the request"))
    return reply.get("result", {})


def call_game(method, timeout=60, **params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    return post("/", body.encode(), "application/json", timeout)


def send_model(model, glb):
    path = "/model/" + urllib.parse.quote(model)
    return post(path, glb, "model/gltf-binary", 60)


def link():
    return bpy.context.window_manager.openzt2_link


def redraw_panels():
    for window in bpy.context.window_manager.windows:
        for area in window.screen.areas:
            if area.type == "VIEW_3D":
                area.tag_redraw()


def poll_game():
    state = link()
    if not state.linked:
        return None
    try:
        status = call_game("status", timeout=0.5)
        state.connected = True
        state.version = status.get("version", "")
        state.selected_object = status.get("selected_object") or ""
    except LinkError:
        state.connected = False
        state.selected_object = ""
    redraw_panels()
    return POLL_SECONDS


def model_collections(scene):
    """Every collection in the scene that holds one OpenZT2 model."""
    return [
        collection
        for collection in scene.collection.children_recursive
        if collection.get("openzt2_model")
    ]


def placement_object(collection):
    return next((obj for obj in collection.objects if obj.get("openzt2_placement")), None)


def sync_scene(scene):
    """Sends each loaded model back to the game, relative to its placement."""
    collections = model_collections(scene)
    if not collections:
        raise LinkError("Load an object from the game first.")
    view_layer = scene.view_layers[0]
    selected = [obj for obj in scene.objects if obj.select_get(view_layer=view_layer)]
    path = os.path.join(tempfile.gettempdir(), "openzt2-link-sync.glb")
    try:
        for collection in collections:
            placement = placement_object(collection)
            placed = placement.matrix_basis.copy() if placement else None
            if placement:
                placement.matrix_basis = Matrix.Identity(4)
            for obj in scene.objects:
                obj.select_set(obj.name in collection.objects and obj != placement, view_layer=view_layer)
            # Blender's glTF exporter writes only to a file.
            with bpy.context.temp_override(scene=scene, view_layer=view_layer):
                bpy.ops.export_scene.gltf(
                    filepath=path,
                    export_format="GLB",
                    use_selection=True,
                    export_apply=True,
                    export_cameras=False,
                    export_lights=False,
                )
            if placement:
                placement.matrix_basis = placed
            with open(path, "rb") as exported:
                send_model(collection["openzt2_model"], exported.read())
    finally:
        if os.path.exists(path):
            os.remove(path)
        for obj in scene.objects:
            obj.select_set(obj in selected, view_layer=view_layer)
    return len(collections)


class OPENZT2_OT_connect(bpy.types.Operator):
    """Keep a link to OpenZT2 and show what is selected in the game"""

    bl_idname = "openzt2.connect"
    bl_label = "Connect"

    def execute(self, context):
        state = link()
        state.linked = not state.linked
        if state.linked:
            bpy.app.timers.register(poll_game)
            poll_game()
            state.message = "" if state.connected else "Waiting for OpenZT2."
        else:
            state.connected = False
            state.message = ""
        return {"FINISHED"}


class OPENZT2_OT_load(bpy.types.Operator):
    """Replace this scene with the object selected in OpenZT2, one collection per model.
Edits synced to the game for its models are discarded"""

    bl_idname = "openzt2.load"
    bl_label = "Load"

    save_first: bpy.props.BoolProperty(
        name="Save changes first", description="Save the .blend file before loading", default=True
    )

    def invoke(self, context, event):
        if bpy.data.is_dirty:
            return context.window_manager.invoke_props_dialog(
                self, title="Unsaved changes", confirm_text="Load"
            )
        return self.execute(context)

    def draw(self, context):
        layout = self.layout
        layout.label(text="Loading replaces everything in this scene.")
        if bpy.data.filepath:
            layout.prop(self, "save_first")
        else:
            layout.label(text="This file has never been saved; its changes will be lost.")

    def execute(self, context):
        state = link()
        if bpy.data.is_dirty and self.save_first and bpy.data.filepath:
            bpy.ops.wm.save_mainfile()
        try:
            loaded = call_game("load_object")
        except LinkError as error:
            state.message = str(error)
            self.report({"ERROR"}, str(error))
            return {"CANCELLED"}
        scene = context.scene
        empty_scene(scene)
        object_collection = bpy.data.collections.new(loaded["object"])
        scene.collection.children.link(object_collection)
        imported = []
        for part in loaded["parts"]:
            name = os.path.splitext(os.path.basename(part["model"]))[0]
            collection = bpy.data.collections.new(name)
            collection["openzt2_model"] = part["model"]
            object_collection.children.link(collection)
            layer = find_layer_collection(context.view_layer.layer_collection, collection)
            context.view_layer.active_layer_collection = layer
            before = set(bpy.data.objects)
            bpy.ops.import_scene.gltf(filepath=part["path"])
            new = [obj for obj in bpy.data.objects if obj not in before]
            for obj in new:
                if obj.name.startswith(PLACEMENT_NODE):
                    obj["openzt2_placement"] = True
                    obj.name = name + " placement"
            imported.extend(new)
            # Parts the game is not showing, such as other building states.
            layer.hide_viewport = not part["visible"]
        context.view_layer.active_layer_collection = context.view_layer.layer_collection
        frame(context, imported)
        state.message = "Loaded " + loaded["object"]
        return {"FINISHED"}


def empty_scene(scene):
    for collection in list(scene.collection.children_recursive):
        bpy.data.collections.remove(collection)
    for obj in list(scene.objects):
        bpy.data.objects.remove(obj, do_unlink=True)
    bpy.data.orphans_purge(do_recursive=True)


def find_layer_collection(layer, collection):
    if layer.collection == collection:
        return layer
    for child in layer.children:
        found = find_layer_collection(child, collection)
        if found:
            return found
    return None


def frame(context, objects):
    if context.window is None:
        return
    for obj in context.view_layer.objects:
        obj.select_set(obj in objects)
    for area in context.window.screen.areas:
        if area.type == "VIEW_3D":
            region = next(region for region in area.regions if region.type == "WINDOW")
            with context.temp_override(area=area, region=region):
                bpy.ops.view3d.view_selected()


class OPENZT2_OT_sync(bpy.types.Operator):
    """Send every loaded model to OpenZT2, replacing it in the running game"""

    bl_idname = "openzt2.sync"
    bl_label = "Sync"

    def execute(self, context):
        state = link()
        try:
            count = sync_scene(context.scene)
        except LinkError as error:
            state.message = str(error)
            self.report({"ERROR"}, str(error))
            return {"CANCELLED"}
        state.message = "Synced {} model{}".format(count, "" if count == 1 else "s")
        return {"FINISHED"}


class OPENZT2_OT_export_mod(bpy.types.Operator):
    """Pack every model synced to OpenZT2 into a mod it loads from its next start"""

    bl_idname = "openzt2.export_mod"
    bl_label = "Export as Mod"

    def execute(self, context):
        state = link()
        try:
            saved = call_game("save_as_mod", name=context.scene.openzt2_mod_name)
        except LinkError as error:
            state.message = str(error)
            self.report({"ERROR"}, str(error))
            return {"CANCELLED"}
        state.message = "Saved " + saved["path"]
        self.report({"INFO"}, state.message)
        return {"FINISHED"}


class OPENZT2_PT_link(bpy.types.Panel):
    bl_label = "OpenZT2"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "OpenZT2"

    def draw(self, context):
        state = link()
        layout = self.layout
        if not state.linked:
            layout.label(text="Not connected", icon="UNLINKED")
        elif state.connected:
            layout.label(text="Connected to OpenZT2 " + state.version, icon="LINKED")
            layout.label(text="Selected: " + (state.selected_object or "nothing"))
        else:
            layout.label(text="Waiting for OpenZT2", icon="TIME")
        layout.operator(
            OPENZT2_OT_connect.bl_idname,
            text="Disconnect" if state.linked else "Connect",
            icon="UNLINKED" if state.linked else "LINKED",
        )
        row = layout.row()
        row.enabled = state.connected and bool(state.selected_object)
        row.operator(OPENZT2_OT_load.bl_idname, icon="IMPORT")
        row = layout.row()
        row.enabled = state.connected and bool(model_collections(context.scene))
        row.operator(OPENZT2_OT_sync.bl_idname, icon="FILE_REFRESH")
        layout.prop(context.scene, "openzt2_sync_on_save")
        layout.separator()
        layout.prop(context.scene, "openzt2_mod_name")
        row = layout.row()
        row.enabled = state.connected
        row.operator(OPENZT2_OT_export_mod.bl_idname, icon="PACKAGE")
        if state.message:
            for line in state.message.split(". "):
                layout.label(text=line)


class OpenZT2LinkState(bpy.types.PropertyGroup):
    linked: bpy.props.BoolProperty()
    connected: bpy.props.BoolProperty()
    version: bpy.props.StringProperty()
    selected_object: bpy.props.StringProperty()
    message: bpy.props.StringProperty()


@persistent
def sync_after_save(_):
    state = link()
    for scene in bpy.data.scenes:
        if scene.openzt2_sync_on_save and state.connected and model_collections(scene):
            try:
                count = sync_scene(scene)
                state.message = "Synced {} model{}".format(count, "" if count == 1 else "s")
            except LinkError as error:
                state.message = str(error)


classes = (
    OpenZT2LinkState,
    OPENZT2_OT_connect,
    OPENZT2_OT_load,
    OPENZT2_OT_sync,
    OPENZT2_OT_export_mod,
    OPENZT2_PT_link,
)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.WindowManager.openzt2_link = bpy.props.PointerProperty(type=OpenZT2LinkState)
    bpy.types.Scene.openzt2_sync_on_save = bpy.props.BoolProperty(
        name="Sync on save", description="Send the models to OpenZT2 each time the file is saved"
    )
    bpy.types.Scene.openzt2_mod_name = bpy.props.StringProperty(
        name="Mod name",
        description="File name of the mod, without .z2f: letters, digits, '-' and '_'",
        default="blend2zoo",
    )
    bpy.app.handlers.save_post.append(sync_after_save)


def unregister():
    if bpy.app.timers.is_registered(poll_game):
        bpy.app.timers.unregister(poll_game)
    bpy.app.handlers.save_post.remove(sync_after_save)
    del bpy.types.Scene.openzt2_mod_name
    del bpy.types.Scene.openzt2_sync_on_save
    del bpy.types.WindowManager.openzt2_link
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
