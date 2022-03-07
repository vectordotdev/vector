###################
# VECTOR TILTFILE #
###################

load('ext://helm_resource', 'helm_resource', 'helm_repo')

docker_build(ref='timberio/vector', context='.', dockerfile='tilt/Dockerfile')

# temporarily use branch with more rbac
k8s_yaml(helm(
    '/Users/spencer.gilbert/Code/vector-helm-charts/charts/vector',
    set=[
        'role=Agent'
    ]
    ))
# helm_repo(name='vectordotdev', url='https://helm.vector.dev')
# helm_resource(
#     name='vector',
#     chart='vectordotdev/vector',
#     image_deps=['timberio/vector'],
#     image_keys=[('image.repository', 'image.tag')],
#     flags=[
#         '--set', 'role=Agent',
#         # '--set', 'env[0].name=VECTOR_LOG',
#         # '--set', 'env[0].value=trace'
#         ]
#     )

k8s_resource(workload='chart-vector', port_forwards=8686)