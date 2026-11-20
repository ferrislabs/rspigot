import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.MapColor;
import net.minecraft.world.level.saveddata.maps.MapDecorationType;

public final class MapColorDump {
    private MapColorDump() {
    }

    public static void main(String[] args) throws IOException {
        if (args.length != 1) {
            System.err.println("usage: MapColorDump <output.json>");
            System.exit(2);
        }
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Path out = Path.of(args[0]).toAbsolutePath();
        Files.createDirectories(out.getParent());
        Path tmp = out.resolveSibling(out.getFileName() + ".tmp");
        try (JsonWriter json = new JsonWriter(Files.newBufferedWriter(tmp, StandardCharsets.UTF_8))) {
            json.setIndent(" ");
            json.beginObject();
            json.name("version").value(SharedConstants.getCurrentVersion().name());
            json.name("brightness").beginArray();
            for (MapColor.Brightness brightness : MapColor.Brightness.values()) {
                json.beginObject();
                json.name("name").value(brightness.name().toLowerCase());
                json.name("id").value(brightness.id);
                json.name("modifier").value(brightness.modifier);
                json.endObject();
            }
            json.endArray();
            json.name("colors").beginArray();
            for (int id = 0; id < 64; id++) {
                MapColor color = MapColor.byId(id);
                json.beginObject();
                json.name("id").value(color.id);
                json.name("rgb").value(color.col);
                json.endObject();
            }
            json.endArray();
            json.name("states").beginArray();
            int expected = 0;
            for (Block block : BuiltInRegistries.BLOCK) {
                for (BlockState state : block.getStateDefinition().getPossibleStates()) {
                    if (Block.getId(state) != expected) {
                        throw new IllegalStateException("state id " + Block.getId(state) + " expected " + expected);
                    }
                    expected++;
                    json.value(state.getMapColor(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).id);
                }
            }
            json.endArray();
            json.name("decoration_types").beginArray();
            for (MapDecorationType type : BuiltInRegistries.MAP_DECORATION_TYPE) {
                json.beginObject();
                json.name("name").value(BuiltInRegistries.MAP_DECORATION_TYPE.getKey(type).toString());
                json.name("id").value(BuiltInRegistries.MAP_DECORATION_TYPE.getId(type));
                json.name("asset").value(type.assetId().toString());
                json.name("show_on_item_frame").value(type.showOnItemFrame());
                json.name("track_count").value(type.trackCount());
                json.endObject();
            }
            json.endArray();
            json.endObject();
        }
        Files.move(tmp, out, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        System.err.println("wrote " + out);
    }
}
