import com.google.gson.stream.JsonWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.lang.reflect.ParameterizedType;
import java.lang.reflect.Type;
import java.util.IdentityHashMap;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.entity.EntityTypes;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityAttachment;
import net.minecraft.world.entity.EntityAttachments;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.phys.Vec3;

public final class AttachmentDump {
    private AttachmentDump() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 1) {
            System.err.println("usage: AttachmentDump <output.json>");
            System.exit(2);
        }
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Field field = EntityAttachments.class.getDeclaredField("attachments");
        field.setAccessible(true);
        Path out = Path.of(args[0]).toAbsolutePath();
        Files.createDirectories(out.getParent());
        try (JsonWriter json = new JsonWriter(Files.newBufferedWriter(out, StandardCharsets.UTF_8))) {
            json.setIndent(" ");
            json.beginObject();
            json.name("version").value(SharedConstants.getCurrentVersion().name());
            json.name("types");
            json.beginObject();
            for (int id = 0; id < BuiltInRegistries.ENTITY_TYPE.size(); id++) {
                EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.byId(id);
                EntityAttachments attachments = type.getDimensions().attachments();
                @SuppressWarnings("unchecked")
                Map<EntityAttachment, List<Vec3>> map = (Map<EntityAttachment, List<Vec3>>) field.get(attachments);
                json.name(EntityType.getKey(type).toString());
                json.beginObject();
                json.name("height").value(Float.valueOf(type.getHeight()));
                json.name("width").value(Float.valueOf(type.getWidth()));
                write(json, "passenger", map.get(EntityAttachment.PASSENGER));
                write(json, "vehicle", map.get(EntityAttachment.VEHICLE));
                json.endObject();
            }
            json.endObject();
            json.name("babies");
            json.beginObject();
            Map<EntityType<?>, Class<?>> classes = entityClasses();
            for (int id = 0; id < BuiltInRegistries.ENTITY_TYPE.size(); id++) {
                EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.byId(id);
                for (Class<?> c = classes.get(type); c != null && c != Entity.class; c = c.getSuperclass()) {
                    EntityDimensions found = null;
                    String name = null;
                    for (Field candidate : c.getDeclaredFields()) {
                        if (Modifier.isStatic(candidate.getModifiers()) && candidate.getType() == EntityDimensions.class
                                && candidate.getName().contains("BABY")) {
                            candidate.setAccessible(true);
                            found = (EntityDimensions) candidate.get(null);
                            name = candidate.getName();
                        }
                    }
                    if (found != null) {
                        @SuppressWarnings("unchecked")
                        Map<EntityAttachment, List<Vec3>> map = (Map<EntityAttachment, List<Vec3>>) field.get(found.attachments());
                        json.name(EntityType.getKey(type).toString());
                        json.beginObject();
                        json.name("field").value(c.getSimpleName() + "." + name);
                        json.name("height").value(Float.valueOf(found.height()));
                        json.name("width").value(Float.valueOf(found.width()));
                        write(json, "passenger", map.get(EntityAttachment.PASSENGER));
                        write(json, "vehicle", map.get(EntityAttachment.VEHICLE));
                        json.endObject();
                        break;
                    }
                }
            }
            json.endObject();
            json.endObject();
        }
        System.err.println("wrote " + out);
    }

    private static Map<EntityType<?>, Class<?>> entityClasses() throws IllegalAccessException {
        Map<EntityType<?>, Class<?>> classes = new IdentityHashMap<>();
        for (Field f : EntityTypes.class.getDeclaredFields()) {
            if (!Modifier.isStatic(f.getModifiers()) || f.getType() != EntityType.class) {
                continue;
            }
            Type generic = f.getGenericType();
            if (generic instanceof ParameterizedType parameterized
                    && parameterized.getActualTypeArguments()[0] instanceof Class<?> entityClass) {
                classes.put((EntityType<?>) f.get(null), entityClass);
            }
        }
        return classes;
    }

    private static void write(JsonWriter json, String name, List<Vec3> points) throws Exception {
        json.name(name);
        json.beginArray();
        if (points != null) {
            for (Vec3 point : points) {
                json.beginArray();
                json.value(point.x());
                json.value(point.y());
                json.value(point.z());
                json.endArray();
            }
        }
        json.endArray();
    }
}
